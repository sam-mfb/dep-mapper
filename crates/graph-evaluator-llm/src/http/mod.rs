//! The contract for sending one HTTP POST and reading the whole response.
//! `UreqTransport` implements it with the `ureq` library.

mod ureq_transport;

use thiserror::Error;
pub use ureq_transport::UreqTransport;

pub trait HttpTransport {
    /// Any status, including an error status, is a response. Only a failure
    /// to get a response is a `TransportError`.
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    /// Header names are lowercase.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpResponse {
    /// The first value of the header with this lowercase name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// No response arrived, for example because the connection failed or timed out.
#[derive(Clone, Debug, Error, PartialEq)]
#[error("{message}")]
pub struct TransportError {
    pub message: String,
}
