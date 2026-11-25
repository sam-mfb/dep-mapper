pub use store::helper as reexported_helper;
use store::Backups;

pub struct Parts {
    pub backups: Box<dyn Backups>,
}

pub fn consume(parts: &Parts) -> u32 {
    parts.backups.write() + clock::now()
}

pub fn file_name() -> &'static str {
    store::DATABASE_FILE
}
