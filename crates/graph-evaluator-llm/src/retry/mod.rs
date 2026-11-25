//! A `ModelClient` that tries again after a transient failure. Waiting is an
//! injected `Pause`, so tests run without delay.

use crate::model::{ModelClient, ModelError, ModelReply, ModelRequest};
use std::time::Duration;

pub trait Pause {
    fn pause(&self, duration: Duration);
}

/// Pauses the current thread.
pub struct ThreadPause;

impl Pause for ThreadPause {
    fn pause(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

pub struct RetryingClient<C, P> {
    inner: C,
    pause: P,
    /// The wait before each retry. Its length is the number of retries.
    delays: Vec<Duration>,
}

impl<C: ModelClient, P: Pause> RetryingClient<C, P> {
    /// When the provider names a longer wait than the next delay, waits that
    /// long instead.
    pub fn new(inner: C, pause: P, delays: Vec<Duration>) -> Self {
        RetryingClient {
            inner,
            pause,
            delays,
        }
    }
}

impl<C: ModelClient, P: Pause> ModelClient for RetryingClient<C, P> {
    fn reply(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        let mut delays = self.delays.iter();
        loop {
            match self.inner.reply(request) {
                Err(ModelError::Transient {
                    message,
                    retry_after,
                }) => match delays.next() {
                    Some(delay) => self
                        .pause
                        .pause(retry_after.map_or(*delay, |after| after.max(*delay))),
                    None => {
                        return Err(ModelError::Transient {
                            message: format!("{message} (after {} retries)", self.delays.len()),
                            retry_after,
                        });
                    }
                },
                other => return other,
            }
        }
    }
}

#[cfg(test)]
mod tests;
