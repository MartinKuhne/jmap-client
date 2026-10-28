//! A hand-rolled, deliberately minimal HTTP/1.1 implementation.
//!
//! The mock only ever needs to understand what `jmap-client` (and a plain
//! `curl`-style test) can send, and to answer with a self-contained response:
//!
//! * request line + headers, `Content-Length` bodies, and `Transfer-Encoding:
//!   chunked` bodies (decoded defensively, even though the client does not use
//!   it today),
//! * responses that always carry `Content-Length` and `Connection: close`, so
//!   every test request is a single round trip and no connection bookkeeping can
//!   leak between tests.
//!
//! Deliberate extension points for later phases:
//! * [`HttpResponse::streaming`] is the hook the EventSource route (P2) will use
//!   to write `text/event-stream` chunks instead of a fixed body.
//! * the WebSocket upgrade (P2) is handled before [`read_request`] ever sees a
//!   body, by taking the raw socket out of the accept loop.

use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Hard cap on the head section (request line + headers).
const MAX_HEAD_SIZE: usize = 1024 * 1024;
/// Hard cap on the body section.
const MAX_BODY_SIZE: usize = 128 * 1024 * 1024;

/// A parsed HTTP request.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// Upper-cased request method, e.g. `GET` / `POST`.
    pub method: String,
    /// Percent-decoded-free request target path, e.g. `/jmap`.
    pub path: String,
    /// Raw query string (without the leading `?`), if any.
    pub query: Option<String>,
    /// Headers, names lower-cased, in wire order.
    pub headers: Vec<(String, String)>,
    /// Fully buffered request body.
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Returns the value of header `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// `true` when the request carries usable `Authorization` credentials.
    ///
    /// RFC 8620 section 3 leaves the authentication scheme to the server; this
    /// mock accepts the two schemes `jmap-client` can be configured with.
    pub fn is_authenticated(&self) -> bool {
        match self.header("authorization") {
            Some(value) => {
                let value = value.trim_start();
                let scheme = value.split_whitespace().next().unwrap_or("");
                scheme.eq_ignore_ascii_case("basic") || scheme.eq_ignore_ascii_case("bearer")
            }
            None => false,
        }
    }
}

/// A response to write back to the socket.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    /// Extra headers appended verbatim (`name, value`).
    pub extra_headers: Vec<(String, String)>,
}

impl HttpResponse {
    /// A JSON response.
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            content_type: "application/json".to_string(),
            body: body.to_string().into_bytes(),
            extra_headers: Vec::new(),
        }
    }

    /// A raw response with an explicit content type.
    pub fn raw(status: u16, content_type: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type: content_type.into(),
            body,
            extra_headers: Vec::new(),
        }
    }

    /// A streaming response skeleton for the EventSource route (P2).
    ///
    /// The body written here is sent first; the caller keeps the socket and keeps
    /// writing chunks. Kept separate from [`HttpResponse::json`] so the P2 work
    /// does not have to change any existing constructor.
    pub fn streaming(status: u16, first_chunk: Vec<u8>) -> Self {
        Self {
            status,
            content_type: "text/event-stream".to_string(),
            body: first_chunk,
            extra_headers: vec![("Cache-Control".to_string(), "no-cache".to_string())],
        }
    }
}

/// Reads one request from the stream.
///
/// Returns `Ok(None)` when the peer closed the connection cleanly before sending
/// anything, which the accept loop treats as "connection finished".
pub async fn read_request(stream: &mut TcpStream) -> io::Result<Option<HttpRequest>> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];
    let head_end = loop {
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() >= MAX_HEAD_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "head too large"));
        }
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            if buf.is_empty() {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed mid-head",
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
    };

    let head = std::str::from_utf8(&buf[..head_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "head is not utf-8"))?;

    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .filter(|line| !line.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;

    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?
        .to_uppercase();
    let target = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing target"))?;

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target.to_string(), None),
    };

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((
                name.trim().to_ascii_lowercase(),
                value.trim().to_string(),
            ));
        }
    }

    let mut body = buf[head_end + 4..].to_vec();

    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked"));

    if chunked {
        read_chunked_body(stream, &mut body).await?;
        let body = decode_chunked(&body)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad chunked body"))?;
        return Ok(Some(HttpRequest {
            method,
            path,
            query,
            headers,
            body,
        }));
    }

    let expected = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    if expected > MAX_BODY_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }

    while body.len() < expected {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed mid-body",
            ));
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(expected);

    Ok(Some(HttpRequest {
        method,
        path,
        query,
        headers,
        body,
    }))
}

async fn read_chunked_body(stream: &mut TcpStream, buf: &mut Vec<u8>) -> io::Result<()> {
    // Read until the terminating zero-length chunk marker is present.
    loop {
        if buf.ends_with(b"\r\n0\r\n\r\n") || buf.ends_with(b"0\r\n\r\n") {
            return Ok(());
        }
        if buf.len() >= MAX_BODY_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
        }
        let mut tmp = [0u8; 8192];
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

/// Decodes a `Transfer-Encoding: chunked` body.
///
/// Returns `None` when the framing is not valid chunked data.
fn decode_chunked(raw: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        let idx = find_subsequence(&raw[pos..], b"\r\n")? + pos;
        let size_line = std::str::from_utf8(&raw[pos..idx]).ok()?;
        let size_str = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_str, 16).ok()?;
        pos = idx + 2;
        if size == 0 {
            return Some(out);
        }
        if pos + size > raw.len() {
            return None;
        }
        out.extend_from_slice(&raw[pos..pos + size]);
        pos += size;
        // Each chunk is followed by CRLF.
        if raw.get(pos..pos + 2) != Some(b"\r\n".as_slice()) {
            return None;
        }
        pos += 2;
    }
}

/// Writes `response` to `stream` and shuts the write half down.
pub async fn write_response(stream: &mut TcpStream, response: &HttpResponse) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n",
        response.status,
        reason_phrase(response.status),
        response.content_type,
        response.body.len()
    );
    let mut head = head.into_bytes();
    for (name, value) in &response.extra_headers {
        head.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    head.extend_from_slice(b"\r\n");

    stream.write_all(&head).await?;
    stream.write_all(&response.body).await?;
    stream.flush().await?;
    let _ = stream.shutdown().await;
    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
