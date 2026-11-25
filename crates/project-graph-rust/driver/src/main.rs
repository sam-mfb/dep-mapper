//! A rustc wrapper for Cargo's `RUSTC_WORKSPACE_WRAPPER`. It compiles each
//! workspace crate as rustc would and, once analysis is done, writes records
//! of what the compiler resolved to a file in `SRS_RECORDS_DIR`.
//!
//! Function bodies are read through `rustc_public`. Items, `use` items, paths
//! and trait impls come from the compiler's private interface, which
//! `rustc_public` does not cover. Both are tied to the pinned toolchain.
#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_public;
extern crate rustc_session;
extern crate rustc_span;

mod bodies;
mod source;

use project_graph_rust_records::{RECORDS_DIR_VAR, Record, write_record};
use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
};

struct Collector {
    records_dir: Option<PathBuf>,
    failure: Option<String>,
}

impl rustc_driver::Callbacks for Collector {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        if self.records_dir.is_some() {
            // Keep every call the source makes: MIR inlining removes some.
            config.opts.unstable_opts.mir_opt_level = Some(0);
        }
    }

    fn after_analysis<'tcx>(
        &mut self,
        _: &rustc_interface::interface::Compiler,
        tcx: rustc_middle::ty::TyCtxt<'tcx>,
    ) -> rustc_driver::Compilation {
        if let Some(dir) = &self.records_dir {
            let mut records = source::collect(tcx);
            records.extend(bodies::collect(tcx));
            if let Err(error) = write_file(dir, &records) {
                self.failure = Some(format!(
                    "cannot write records to {}: {error}",
                    dir.display()
                ));
                return rustc_driver::Compilation::Stop;
            }
        }
        rustc_driver::Compilation::Continue
    }
}

fn write_file(dir: &std::path::Path, records: &[Record]) -> std::io::Result<()> {
    let Some(Record::Unit(unit)) = records.first() else {
        return Err(std::io::Error::other("no unit record"));
    };
    let name = format!(
        "{}-{}{}.jsonl",
        unit.crate_name,
        unit.crate_id,
        if unit.test { "-test" } else { "" }
    );
    let mut out = BufWriter::new(File::create(dir.join(name))?);
    for record in records {
        write_record(&mut out, record)?;
    }
    out.flush()
}

fn main() -> ExitCode {
    // Cargo passes the real rustc path first; rustc_driver treats it as argv[0].
    let args: Vec<String> = env::args().skip(1).collect();
    // Build scripts are compiled through the wrapper too; they are not part of
    // the analyzed build.
    let build_script = env::var("CARGO_CRATE_NAME").is_ok_and(|n| n.starts_with("build_script_"));
    let records_dir = env::var_os(RECORDS_DIR_VAR)
        .filter(|_| !build_script)
        .map(PathBuf::from);
    let mut collector = Collector {
        records_dir,
        failure: None,
    };
    let code =
        rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut collector));
    match collector.failure {
        Some(message) => {
            eprintln!("srs-rustc-driver: {message}");
            ExitCode::FAILURE
        }
        None => code,
    }
}
