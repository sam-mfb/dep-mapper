//! A live preview of a map page in the browser. The caller shows each newly
//! built page, or the problems that kept one from being built. The preview
//! keeps the last good page, lists the problems beside it, and tells open
//! browsers to reload. Building pages, and deciding when to rebuild, belong
//! to the caller; `Watcher` reports when files it reads have changed.
//!
//! The preview serves an outer page that holds the map page in a frame and
//! listens for changes, so the map page itself makes no network requests.

mod outer;
mod server;
mod shown;
mod watch;

pub use server::{Preview, PreviewError, Settings};
pub use shown::Shown;
pub use watch::{PollingWatcher, WatchError, Watcher};
