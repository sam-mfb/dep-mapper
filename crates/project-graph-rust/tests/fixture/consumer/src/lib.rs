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

mod inner {
    // `pub`, but other crates cannot reach it through a private module.
    #[allow(unused_imports)]
    pub use store::helper as inner_helper;
}

pub fn make<T: store::Build>(seed: u32) -> T {
    T::build(seed)
}

// What serde's derives generate: the dependency under a local alias.
const _: () = {
    extern crate clock as _clock;
    impl Parts {
        pub fn stamp() -> u32 {
            _clock::now()
        }
    }
};

/// Uses the supplier inside an `async fn`, whose body is a coroutine that
/// captures its arguments.
pub async fn write_later(backups: Box<dyn Backups>) -> u32 {
    backups.write()
}

/// Receives the supplier wrapped in an `Option`, and takes it out through
/// the standard library.
pub fn write_some(backups: Option<Box<dyn Backups>>) -> u32 {
    backups.unwrap().write()
}
