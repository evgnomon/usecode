// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Minimal HTTP client for the NATS monitoring endpoint on localhost.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(5);

/// GET `http://localhost:{port}{path}`; returns (status, body).
fn get(port: i64, path: &str) -> Option<(u16, Vec<u8>)> {
    let port = u16::try_from(port).ok()?;
    let addrs = ("localhost", port).to_socket_addrs().ok()?;
    let mut stream = addrs
        .into_iter()
        .find_map(|addr| TcpStream::connect_timeout(&addr, TIMEOUT).ok())?;
    stream.set_read_timeout(Some(TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(TIMEOUT)).ok()?;
    // HTTP/1.0 keeps the response un-chunked and closes the connection.
    let req = format!("GET {path} HTTP/1.0\r\nHost: localhost:{port}\r\nAccept: */*\r\n\r\n");
    stream.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    parse_response(&buf)
}

/// Split a raw HTTP response into status code and body.
fn parse_response(buf: &[u8]) -> Option<(u16, Vec<u8>)> {
    let head_end = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&buf[..head_end]).ok()?;
    let status = head.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, buf[head_end + 4..].to_vec()))
}

/// True if `/healthz` answers 200.
pub fn node_healthy(monitoring_port: i64) -> bool {
    matches!(get(monitoring_port, "/healthz"), Some((200, _)))
}

/// Parsed `/routez` JSON, if the node answers with a 2xx and valid JSON.
pub fn route_info(monitoring_port: i64) -> Option<Value> {
    let (status, body) = get(monitoring_port, "/routez")?;
    if !(200..300).contains(&status) {
        return None;
    }
    serde_json::from_slice(&body).ok()
}

/// `num_routes` of a `/routez` document (0 when missing or empty).
pub fn num_routes(info: Option<&Value>) -> Value {
    info.and_then(Value::as_object)
        .filter(|o| !o.is_empty())
        .and_then(|o| o.get("num_routes"))
        .cloned()
        .unwrap_or(Value::from(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_response() {
        let raw = b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"num_routes\":2}";
        let (status, body) = parse_response(raw).unwrap();
        assert_eq!(status, 200);
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(num_routes(Some(&v)), Value::from(2));
        assert!(parse_response(b"garbage").is_none());
    }

    fn serve_once(response: &'static [u8]) -> i64 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = conn.read(&mut buf);
            conn.write_all(response).unwrap();
        });
        i64::from(port)
    }

    #[test]
    fn talks_http() {
        let port = serve_once(b"HTTP/1.0 200 OK\r\n\r\n{\"num_routes\":1}");
        let info = route_info(port);
        assert_eq!(num_routes(info.as_ref()), Value::from(1));
        let port = serve_once(b"HTTP/1.0 503 Service Unavailable\r\n\r\n{}");
        assert!(!node_healthy(port));
        let port = serve_once(b"HTTP/1.0 200 OK\r\n\r\n{\"status\":\"ok\"}");
        assert!(node_healthy(port));
    }

    #[test]
    fn num_routes_defaults() {
        assert_eq!(num_routes(None), Value::from(0));
        assert_eq!(num_routes(Some(&serde_json::json!({}))), Value::from(0));
        assert_eq!(
            num_routes(Some(&serde_json::json!({"x": 1}))),
            Value::from(0)
        );
    }
}
