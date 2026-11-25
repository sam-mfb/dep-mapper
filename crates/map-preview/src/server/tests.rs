use super::*;
use std::io::Read;

fn start() -> Preview {
    let settings = Settings {
        heartbeat: Duration::from_millis(50),
        on_connection_error: Arc::new(|_| {}),
    };
    Preview::start("127.0.0.1:0", settings).expect("a free local port")
}

/// The status line and body of a plain request.
fn get(preview: &Preview, path: &str) -> (String, String) {
    let mut stream = TcpStream::connect(preview.address()).unwrap();
    write!(stream, "GET {path} HTTP/1.1\r\nHost: test\r\n\r\n").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_string(), body.to_string())
}

/// Reads event-stream lines until the next event's data.
fn next_event(reader: &mut BufReader<TcpStream>) -> serde_json::Value {
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if let Some(data) = line.strip_prefix("data: ") {
            return serde_json::from_str(data.trim_end()).unwrap();
        }
    }
}

#[test]
fn the_outer_page_holds_the_map_in_a_frame() {
    let preview = start();
    let (status, body) = get(&preview, "/");
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert!(body.contains("<iframe id=\"map\""));
    assert!(body.contains("new EventSource(\"/events\")"));
}

#[test]
fn the_map_is_the_last_good_page() {
    let preview = start();
    assert!(
        get(&preview, "/map")
            .1
            .contains("No map has been built yet.")
    );
    preview.show_page("<p>one</p>".into());
    preview.show_problems(vec!["bad".into()]);
    let (status, body) = get(&preview, "/map?v=2#module=a");
    assert_eq!(
        (status.as_str(), body.as_str()),
        ("HTTP/1.1 200 OK", "<p>one</p>")
    );
    assert_eq!(get(&preview, "/elsewhere").0, "HTTP/1.1 404 Not Found");
}

#[test]
fn a_browser_hears_of_every_change() {
    let preview = start();
    let mut stream = TcpStream::connect(preview.address()).unwrap();
    write!(stream, "GET /events HTTP/1.1\r\nHost: test\r\n\r\n").unwrap();
    let mut reader = BufReader::new(stream);
    assert_eq!(
        next_event(&mut reader),
        serde_json::json!({ "version": 0, "page": null, "problems": [] })
    );
    preview.show_page("<p>one</p>".into());
    assert_eq!(next_event(&mut reader)["version"], 1);
    preview.show_problems(vec!["3:7: unknown role".into()]);
    assert_eq!(
        next_event(&mut reader),
        serde_json::json!({ "version": 2, "page": 1, "problems": ["3:7: unknown role"] })
    );
}

#[test]
fn an_address_in_use_is_reported() {
    let preview = start();
    let settings = Settings {
        heartbeat: Duration::from_millis(50),
        on_connection_error: Arc::new(|_| {}),
    };
    let address = preview.address().to_string();
    let error = Preview::start(&address, settings)
        .err()
        .expect("the port is taken");
    assert_eq!(error.address, address);
}
