use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;

/// Serves one HTTP response on a local port and returns the URL and a handle
/// that yields the request text it received.
fn serve_once(response: &'static str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/messages", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut head = String::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
            let end = line == "\r\n";
            head.push_str(&line);
            if end {
                break;
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let mut stream = stream;
        stream.write_all(response.as_bytes()).unwrap();
        head + &String::from_utf8(body).unwrap()
    });
    (url, handle)
}

fn request(url: String) -> HttpRequest {
    HttpRequest {
        url,
        headers: vec![("x-api-key".into(), "secret".into())],
        body: "{\"hello\":1}".into(),
    }
}

#[test]
fn sends_headers_and_body_and_reads_the_response() {
    let (url, server) = serve_once(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nRetry-After: 3\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );
    let response = UreqTransport::new(Duration::from_secs(10))
        .post(&request(url))
        .unwrap();
    let received = server.join().unwrap();
    assert!(received.starts_with("POST /v1/messages"), "{received}");
    assert!(
        received.to_lowercase().contains("x-api-key: secret"),
        "{received}"
    );
    assert!(received.ends_with("{\"hello\":1}"), "{received}");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, "{}");
    assert_eq!(response.header("retry-after"), Some("3"));
}

#[test]
fn an_error_status_is_a_response() {
    let (url, server) = serve_once(
        "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 4\r\nConnection: close\r\n\r\nslow",
    );
    let response = UreqTransport::new(Duration::from_secs(10))
        .post(&request(url))
        .unwrap();
    server.join().unwrap();
    assert_eq!(response.status, 429);
    assert_eq!(response.body, "slow");
}

#[test]
fn no_listener_is_a_transport_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    drop(listener);
    assert!(
        UreqTransport::new(Duration::from_secs(5))
            .post(&request(url))
            .is_err()
    );
}
