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

/// Uses a type `clock` re-exports from its own dependency, `tick`.
pub fn first_tick() -> clock::Tick {
    clock::Tick(1)
}
