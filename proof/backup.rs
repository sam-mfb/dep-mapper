//! A bounded proof over the pinned compiler's unoptimized, typed function bodies.
use std::{collections::BTreeSet, env, fs, path::PathBuf};

#[derive(Debug, PartialEq)]
struct Assignment<'a> {
    place: &'a str,
    value: &'a str,
    span: &'a str,
}

impl Assignment<'_> {
    fn call(&self) -> Option<(&str, Vec<&str>)> {
        // The selected calls take locals or function items. Reject composite operands.
        let (invocation, _) = self.value.split_once(") -> [")?;
        let (callee, actuals) = invocation.rsplit_once('(')?;
        Some((callee, actuals.split(", ").collect()))
    }

    fn arg(&self, index: usize) -> &str {
        self.call().expect("expected a call").1[index]
    }

    fn field(&self, index: usize) -> (&str, &str) {
        let (_, fields) = self
            .value
            .rsplit_once(" { ")
            .expect("expected an aggregate");
        fields
            .strip_suffix(" }")
            .unwrap()
            .split(", ")
            .nth(index)
            .unwrap()
            .split_once(": ")
            .unwrap()
    }
}

struct Body(String);

impl Body {
    fn open(target: &str, item: &str) -> Self {
        let root = PathBuf::from(env::var_os("PROOF_DUMPS").unwrap());
        let crate_name = target.split('-').next().unwrap();
        let path = root
            .join(target)
            .join(format!("{crate_name}.{item}.1-1-000.built.after.mir"));
        Self(fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
    }

    fn assignments(&self) -> impl Iterator<Item = Assignment<'_>> {
        self.0.lines().filter_map(|line| {
            let (instruction, comment) = line.trim().split_once("; ")?;
            let span = comment.split_once(" at ")?.1;
            let (place, value) = instruction.split_once(" = ")?;
            Some(Assignment { place, value, span })
        })
    }

    fn one(&self, reason: &str, predicate: impl Fn(&Assignment) -> bool) -> Assignment<'_> {
        let mut found = self.assignments().filter(predicate);
        let result = found.next().unwrap_or_else(|| panic!("missing {reason}"));
        assert!(found.next().is_none(), "ambiguous {reason}");
        println!("  {reason} @ {}", result.span);
        result
    }

    fn call(&self, name: &str) -> Assignment<'_> {
        self.one(name, |a| {
            a.call().is_some_and(|(callee, _)| {
                callee == name || callee.starts_with(&format!("{name}::<"))
            })
        })
    }

    fn method(&self, receiver: &str, method: &str) -> Assignment<'_> {
        self.one(method, |a| {
            a.call().is_some_and(|(callee, _)| {
                callee.starts_with(receiver) && callee.ends_with(&format!("::{method}"))
            })
        })
    }

    // Follow only unique definitions, moves, borrows and the two coercions used here.
    // Calls and projections are evidence boundaries checked separately by each query.
    fn origin<'a>(&'a self, mut value: &'a str) -> &'a str {
        let mut seen = BTreeSet::new();
        loop {
            if let Some((operand, _)) = value.split_once(" as ")
                && value.contains("(PointerCoercion(")
            {
                value = operand;
            }
            value = value
                .strip_prefix("move ")
                .or_else(|| value.strip_prefix("copy "))
                .unwrap_or(value);
            value = value.strip_prefix('&').unwrap_or(value);
            if let Some(inner) = value.strip_prefix("(*").and_then(|v| v.strip_suffix(')')) {
                value = inner;
            }
            if !value.strip_prefix('_').is_some_and(|digits| {
                !digits.is_empty() && digits.bytes().all(|d| d.is_ascii_digit())
            }) {
                return value;
            }
            assert!(seen.insert(value), "cycle at {value}");
            let mut defs = self.assignments().filter(|a| a.place == value);
            let Some(def) = defs.next() else { return value };
            assert!(defs.next().is_none(), "ambiguous definition of {value}");
            value = def.value;
        }
    }

    fn returned(&self) -> Assignment<'_> {
        self.one("returned value", |a| a.place == "_0")
    }
}

fn program(item: &str) -> Body {
    Body::open("kashkeshet-lib", item)
}
fn install(item: &str) -> Body {
    Body::open("kkt_install_commands-lib", item)
}
fn writer(item: &str) -> Body {
    Body::open("kkt_backup_os-lib", item)
}

// MIR represents a field by its zero-based declaration index. Use the index from
// the receiver read to select the named field in the producer's aggregate.
fn projection(value: &str, base: &str) -> usize {
    let (place, _) = value.strip_prefix('(').unwrap().split_once(": ").unwrap();
    let (receiver, field) = place.rsplit_once('.').unwrap();
    assert_eq!(receiver, base);
    field.parse().unwrap()
}

#[test]
fn injected_writer_from_use_to_definition() {
    let consumer = install("lifecycle-local_backup");
    let write = consumer.method("<dyn kkt_backup::Backups as kkt_backup::Backups>", "write");
    let field = projection(consumer.origin(write.arg(0)), "(*_1)");

    let dispatch = Body::open("kkt-bin", "run-{closure#0}");
    let build = dispatch.method(
        "kashkeshet::machine::native::NativeParts::",
        "install_parts",
    );
    for command in ["backup", "upgrade"] {
        let delivery = dispatch.call(&format!("kkt_install_commands::lifecycle::{command}"));
        assert_eq!(dispatch.origin(delivery.arg(0)), build.value);
        let entry = install(&format!("lifecycle-{command}"));
        if command == "backup" {
            let call = entry.call("lifecycle::local_backup");
            assert_eq!(entry.origin(call.arg(0)), "_1");
            assert_arm(&call, "backup::BackupOf::Local", "local_backup");
        } else {
            let future = entry.returned();
            let body = install("lifecycle-upgrade-{closure#0}");
            let call = body.call("lifecycle::local_backup");
            let capture = projection(body.origin(call.arg(0)), "_1");
            let (name, actual) = future.field(capture);
            assert_eq!((name, entry.origin(actual)), ("parts", "_1"));
            assert_arm(&call, "kkt_deployment::Replace::Service", "local_backup");
        }
    }

    let builder = program("machine-native-{impl#1}-install_parts");
    let init = builder.returned();
    assert!(
        init.value
            .starts_with("kkt_install_commands::InstallParts::")
    );
    let (name, operand) = init.field(field);
    assert_eq!(name, "backups");
    let reference = builder.origin(operand);
    let (allocation, ty) = reference
        .strip_prefix("const {")
        .unwrap()
        .split_once(": ")
        .unwrap();
    assert!(ty.starts_with("&kkt_backup_os::OsBackups<"));
    let allocation_record = builder
        .0
        .lines()
        .find(|l| l.starts_with(&format!("{allocation} (static: ")))
        .unwrap();
    let symbol = allocation_record
        .split_once("static: ")
        .unwrap()
        .1
        .split_once(',')
        .unwrap()
        .0;
    assert_eq!(symbol, "machine::native::BACKUPS");
    let initializer = program(&symbol.replace("::", "-"));
    let constructor = initializer.method("kkt_backup_os::OsBackups::", "new");
    assert_eq!(constructor.place, "_0");
    assert_eq!(
        initializer.origin(constructor.arg(0)),
        "machine::database::copy"
    );
    println!("  InstallParts.{name} -> {symbol} -> OsBackups::new(machine::database::copy)");
}

// At this pin each tested arm contains exactly one call to local_backup across
// the whole function. Locate its compiled span inside the named Rust match arm.
fn assert_arm(call: &Assignment, arm: &str, callee: &str) {
    let mut location = call.span.split(':');
    let source = fs::read_to_string(location.next().unwrap()).unwrap();
    let line: usize = location.next().unwrap().parse().unwrap();
    let lines: Vec<_> = source.lines().collect();
    assert!(lines[line - 1].contains(callee));
    let enclosing = lines[..line]
        .iter()
        .rev()
        .find(|line| {
            line.starts_with("        ") && !line.starts_with("         ") && line.contains("=>")
        })
        .unwrap();
    assert!(enclosing.trim_start().starts_with(arm), "{enclosing}");
}

#[test]
fn serve_constructs_its_own_receivers() {
    program("commands-serve-serve-{closure#0}").call("commands::serve::backup_before_upgrade");
    program("commands-serve-backup_before_upgrade").call("commands::serve::write_backup");
    for (body, operation) in [
        ("write_backup", "write"),
        ("backup_before_upgrade", "prune"),
    ] {
        let body = program(&format!("commands-serve-{body}"));
        let call = body.method("<kkt_backup_os::OsBackups<", operation);
        assert!(call.call().unwrap().0.contains(" as kkt_backup::Backups>"));
        let new = body.method("kkt_backup_os::OsBackups::", "new");
        assert_eq!(body.origin(call.arg(0)), new.value);
        assert_eq!(body.origin(new.arg(0)), "machine::database::copy");
    }
}

#[test]
fn callback_is_stored_forwarded_and_invoked() {
    let fill = writer("fill");
    let invocation = fill.method("<impl FnOnce(", "call_once");
    assert_eq!(fill.origin(invocation.arg(0)), "_3");
    let free_write = writer("write");
    let fill_call = free_write.call("fill");
    assert_eq!(free_write.origin(fill_call.arg(2)), "_3");
    let implementation = writer("{impl#1}-write");
    let helper = implementation.call("write");
    let index = projection(implementation.origin(helper.arg(2)), "(*_1)");
    let constructor = writer("{impl#0}-new");
    let result = constructor.returned();
    let (field, actual) = result.field(index);
    assert_eq!((field, constructor.origin(actual)), ("copy_database", "_1"));
    println!(
        "  OsBackups::new argument 1 -> self.{field} -> write argument 3 -> fill argument 3 -> invoked"
    );
    assert!(fill.0.contains("const kkt_backup::DATABASE_FILE"));
    assert!(
        implementation
            .0
            .lines()
            .any(|l| l.starts_with("fn ") && l.contains("&kkt_backup::Layout"))
    );
    let prune = writer("prune");
    prune.call("kkt_backup::old_names");
    let prune_impl = writer("{impl#1}-prune");
    prune_impl.call("prune");
    assert!(![prune, prune_impl].iter().any(|b| {
        b.assignments()
            .any(|a| a.value.contains("::call_once(") || a.value.contains("((*_1).0: C)"))
    }));
}
