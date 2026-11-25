use super::*;
use std::io::Read;

#[test]
fn every_request_gets_the_page() {
    let server = PageServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = server.address().unwrap();
    std::thread::spawn(move || server.serve("<p>map</p>", &mut |_| {}));
    for path in ["/", "/anything"] {
        let mut stream = TcpStream::connect(address).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Content-Length: 10\r\n"));
        assert!(response.ends_with("\r\n\r\n<p>map</p>"));
    }
}
