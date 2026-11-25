use consumer::{Parts, consume};
use supplier::{Backups, OsBackups};

fn main() {
    let parts = Parts {
        backups: Box::new(OsBackups),
    };
    let direct = OsBackups.write();
    let run = |p: &Parts| consume(p);
    println!("{} {} {}", run(&parts), direct, consumer::file_name());
}
