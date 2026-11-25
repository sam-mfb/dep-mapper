use consumer::{Parts, consume};
use supplier::{Backups, OsBackups};

fn main() {
    let parts = Parts {
        backups: Box::new(OsBackups),
    };
    let direct = OsBackups.write();
    let wrapped = consumer::write_some(Some(Box::new(OsBackups)));
    let generic = consumer::write_generic(Some(OsBackups));
    let lent = consumer::with_backups(&OsBackups, |b| b.write());
    let mapped = Some(Box::new(OsBackups) as Box<dyn Backups>).map(|b| b.write());
    let counted = consumer::with_count(&OsBackups, |n, b| n + b.write());
    let all: [Box<dyn Backups>; 1] = [Box::new(OsBackups)];
    let folded = all.iter().fold(0, |total, b| total + b.write());
    let built = consumer::build_with(|| Box::new(OsBackups));
    let _later = consumer::write_later(Box::new(OsBackups));
    let run = |p: &Parts| consume(p);
    println!(
        "{} {} {} {} {} {:?} {} {} {} {}",
        run(&parts),
        direct,
        wrapped,
        generic,
        lent,
        mapped,
        counted,
        folded,
        built,
        consumer::file_name()
    );
}
