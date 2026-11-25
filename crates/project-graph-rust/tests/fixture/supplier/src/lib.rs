pub trait Backups {
    fn write(&self) -> u32;
}

pub struct OsBackups;

impl Backups for OsBackups {
    fn write(&self) -> u32 {
        helper() + 1
    }
}

pub fn helper() -> u32 {
    41
}

pub const DATABASE_FILE: &str = "db.sqlite";
