//! `HttpTransport` over a blocking `ureq` agent.

use super::{HttpRequest, HttpResponse, HttpTransport, TransportError};
use std::time::Duration;

pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    /// `timeout` bounds each request from connection to the last byte of the
    /// response.
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        UreqTransport {
            agent: config.into(),
        }
    }
}

impl HttpTransport for UreqTransport {
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let mut builder = self.agent.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let mut response = builder
            .send(request.body.as_str())
            .map_err(|error| TransportError {
                message: error.to_string(),
            })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_lowercase(), value.to_string()))
            })
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_string()
            .map_err(|error| TransportError {
                message: format!("reading the response body: {error}"),
            })?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests;
