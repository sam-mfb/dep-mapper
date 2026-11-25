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
    let _later = consumer::write_later(Box::new(OsBackups));
    let run = |p: &Parts| consume(p);
    println!(
        "{} {} {} {} {} {:?} {}",
        run(&parts),
        direct,
        wrapped,
        generic,
        lent,
        mapped,
        consumer::file_name()
    );
}
