//! A small HTTP/1.1 server on 127.0.0.1 for Claude Code's HTTP hooks. Each hook call holds its
//! connection open until the Island answers, skips, or the wait runs out.

use std::{
    io::{ErrorKind, Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use super::{
    logic::{self, PERMISSION, QUESTION},
    AskState,
};

const FILE: &str = "ask-server.json";
const DEFAULT_PORT: u16 = 47821;
/// The default port and the 20 after it.
const PORT_SPAN: u16 = 21;
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(500);
const ISLAND: &str = "island";

/// Port and secret in the URLs of the installed hooks. No `Debug`, so the token cannot end up in a log.
#[derive(Serialize, Deserialize, Clone)]
pub struct Info {
    pub port: u16,
    pub token: String,
}

// ---- HTTP ----

#[derive(Debug, PartialEq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub enum ReadError {
    Bad,
    TooLarge,
}

impl ReadError {
    fn status(&self) -> u16 {
        match self {
            Self::Bad => 400,
            Self::TooLarge => 413,
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Reads one request: the head, then exactly `Content-Length` bytes of body.
pub fn read_request<R: Read>(r: &mut R) -> Result<Request, ReadError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = find(&buf, b"\r\n\r\n") {
            break i;
        }
        if buf.len() > MAX_HEAD {
            return Err(ReadError::TooLarge);
        }
        match r.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(ReadError::Bad),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = std::str::from_utf8(&buf[..head_end]).map_err(|_| ReadError::Bad)?;
    let mut lines = head.split("\r\n");
    let mut start = lines.next().ok_or(ReadError::Bad)?.split(' ');
    let (method, path, version) = (start.next(), start.next(), start.next());
    let (Some(method), Some(path), Some(version)) = (method, path, version) else { return Err(ReadError::Bad) };
    if !version.starts_with("HTTP/1.") {
        return Err(ReadError::Bad);
    }
    let mut length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { return Err(ReadError::Bad) };
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().map_err(|_| ReadError::Bad)?;
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(ReadError::Bad);
        }
    }
    if length > MAX_BODY {
        return Err(ReadError::TooLarge);
    }
    let mut body = buf[head_end + 4..].to_vec();
    body.truncate(length);
    while body.len() < length {
        match r.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(ReadError::Bad),
            Ok(n) => body.extend_from_slice(&chunk[..n.min(length - body.len())]),
        }
    }
    Ok(Request { method: method.to_string(), path: path.to_string(), body })
}

/// The request kind a POST to `/hook/<token>/ask` or `/permission` stands for. Anything else, a wrong token included, is `None`.
pub fn route(request: &Request, token: &str) -> Option<&'static str> {
    if request.method != "POST" {
        return None;
    }
    let path = request.path.split(['?', '#']).next().unwrap_or_default();
    let mut parts = path.strip_prefix('/')?.split('/');
    let (hook, given, tail, rest) = (parts.next()?, parts.next()?, parts.next()?, parts.next());
    if hook != "hook" || given != token || rest.is_some() {
        return None;
    }
    match tail {
        "ask" => Some(QUESTION),
        "permission" => Some(PERMISSION),
        _ => None,
    }
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(body.as_bytes())).and_then(|_| stream.flush());
    let _ = stream.shutdown(Shutdown::Write);
}

// ---- Token and port ----

fn random_token() -> String {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 16];
    ring::rand::SystemRandom::new().fill(&mut bytes).expect("the system's random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn valid_token(t: &str) -> bool {
    t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit())
}

fn file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE))
}

fn load(app: &AppHandle) -> Option<Info> {
    serde_json::from_slice(&std::fs::read(file(app)?).ok()?).ok()
}

fn save(app: &AppHandle, info: &Info) {
    let Some(file) = file(app) else { return };
    if let (Some(dir), Ok(bytes)) = (file.parent(), serde_json::to_vec(info)) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&file, bytes);
    }
}

/// The saved port first, so installed hooks keep working, then the default range.
fn bind(saved: Option<u16>) -> Option<TcpListener> {
    saved
        .into_iter()
        .chain(DEFAULT_PORT..DEFAULT_PORT + PORT_SPAN)
        .find_map(|port| TcpListener::bind(("127.0.0.1", port)).ok())
}

pub fn start(app: &AppHandle) {
    let saved = load(app);
    let token = saved.as_ref().map(|i| i.token.clone()).filter(|t| valid_token(t)).unwrap_or_else(random_token);
    let Some(listener) = bind(saved.as_ref().map(|i| i.port)) else { return };
    let Ok(addr) = listener.local_addr() else { return };
    let info = Info { port: addr.port(), token: token.clone() };
    save(app, &info);
    *app.state::<AskState>().info.lock().unwrap() = Some(info);

    let app = app.clone();
    let _ = thread::Builder::new().name("ask-server".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let (app, token) = (app.clone(), token.clone());
            let _ = thread::Builder::new().name("ask-hook".into()).spawn(move || handle(&app, stream, &token));
        }
    });
}

// ---- One hook call ----

enum Waited {
    Reply(Value),
    Closed,
    TimedOut,
}

/// The agent hung up (Claude Code was interrupted or the question answered in the terminal).
fn peer_closed(stream: &TcpStream) -> bool {
    match stream.peek(&mut [0u8; 1]) {
        Ok(n) => n == 0,
        Err(e) => e.kind() != ErrorKind::WouldBlock,
    }
}

fn wait(stream: &TcpStream, rx: &Receiver<Value>, limit: Duration) -> Waited {
    let _ = stream.set_nonblocking(true);
    let deadline = Instant::now() + limit;
    let waited = loop {
        match rx.recv_timeout(POLL) {
            Ok(reply) => break Waited::Reply(reply),
            Err(RecvTimeoutError::Disconnected) => break Waited::TimedOut,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if peer_closed(stream) {
            break Waited::Closed;
        }
        if Instant::now() >= deadline {
            break Waited::TimedOut;
        }
    };
    let _ = stream.set_nonblocking(false);
    waited
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn handle(app: &AppHandle, mut stream: TcpStream, token: &str) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let request = match read_request(&mut stream) {
        Ok(r) => r,
        Err(e) => return respond(&mut stream, e.status(), "{}"),
    };
    let config = crate::island::config(app);
    let kind = match route(&request, token) {
        Some(k) if config.enabled && config.ask_in_island => k,
        // The body says which check failed, so a hook that does nothing can be told from one the wrong path or setting turned away.
        Some(_) => return respond(&mut stream, 404, r#"{"reason":"off"}"#),
        None => return respond(&mut stream, 404, r#"{"reason":"path"}"#),
    };
    let Ok(body) = serde_json::from_slice::<Value>(&request.body) else { return respond(&mut stream, 400, "{}") };

    let state = app.state::<AskState>();
    let id = state.next_id();
    let Some(ask) = logic::new_ask(id, kind, &body, now_ms()) else { return respond(&mut stream, 200, "{}") };
    let (tx, rx) = mpsc::channel();
    state.add(ask.clone(), tx);
    let _ = app.emit_to(ISLAND, "ask://new", &ask);
    let _ = app.emit_to(ISLAND, "island://command", "module:ask");

    match wait(&stream, &rx, WAIT) {
        Waited::Reply(reply) => respond(&mut stream, 200, &reply.to_string()),
        Waited::Closed => {
            state.take(id);
            let _ = app.emit_to(ISLAND, "ask://gone", id);
        }
        Waited::TimedOut => {
            // An answer can land between the last poll and here.
            if state.take(id).is_none() {
                if let Ok(reply) = rx.try_recv() {
                    return respond(&mut stream, 200, &reply.to_string());
                }
            }
            let _ = app.emit_to(ISLAND, "ask://gone", id);
            respond(&mut stream, 200, &logic::skip_reply(kind).to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn post(path: &str, body: &str) -> Vec<u8> {
        format!("POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    fn read(bytes: &[u8]) -> Result<Request, ReadError> {
        read_request(&mut Cursor::new(bytes.to_vec()))
    }

    #[test]
    fn a_post_with_a_body_is_read_in_full() {
        let r = read(&post("/hook/abc/ask", "{\"a\":1}")).unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str()), ("POST", "/hook/abc/ask"));
        assert_eq!(r.body, b"{\"a\":1}");
    }

    /// Hands out a few bytes per read, like a socket does.
    struct Trickle(Vec<u8>, usize);
    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = 5usize.min(buf.len()).min(self.0.len() - self.1);
            buf[..n].copy_from_slice(&self.0[self.1..self.1 + n]);
            self.1 += n;
            Ok(n)
        }
    }

    #[test]
    fn a_request_arriving_in_pieces_is_read_in_full() {
        let body = "x".repeat(10_000);
        let r = read_request(&mut Trickle(post("/hook/t/permission", &body), 0)).unwrap();
        assert_eq!(r.body.len(), 10_000);
    }

    #[test]
    fn extra_bytes_after_the_body_are_ignored() {
        let mut bytes = post("/p", "abc");
        bytes.extend_from_slice(b"GET / HTTP/1.1\r\n\r\n");
        assert_eq!(read(&bytes).unwrap().body, b"abc");
    }

    #[test]
    fn limits_and_bad_requests_are_refused() {
        let big = format!("POST /p HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1);
        assert_eq!(read(big.as_bytes()), Err(ReadError::TooLarge));
        let long_head = format!("POST /p HTTP/1.1\r\nX: {}\r\n", "a".repeat(MAX_HEAD + 100));
        assert_eq!(read(long_head.as_bytes()), Err(ReadError::TooLarge));
        assert_eq!(read(b"POST /p HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc"), Err(ReadError::Bad));
        assert_eq!(read(b"POST /p HTTP/1.1\r\nContent-Length: ten\r\n\r\n"), Err(ReadError::Bad));
        assert_eq!(read(b"POST /p HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"), Err(ReadError::Bad));
        assert_eq!(read(b"GARBAGE\r\n\r\n"), Err(ReadError::Bad));
        assert_eq!(read(b"POST /p SPDY/3\r\n\r\n"), Err(ReadError::Bad));
        assert_eq!(read(b""), Err(ReadError::Bad));
        assert_eq!(read(b"POST /p HTTP/1.1\r\nContent-Le"), Err(ReadError::Bad));
        assert_eq!(read(b"POST /p HTTP/1.1\r\n\r\n").unwrap().body, b"");
        assert_eq!((ReadError::Bad.status(), ReadError::TooLarge.status()), (400, 413));
    }

    #[test]
    fn only_posts_with_the_right_token_reach_a_handler() {
        let req = |method: &str, path: &str| Request { method: method.into(), path: path.into(), body: Vec::new() };
        assert_eq!(route(&req("POST", &format!("/hook/{TOKEN}/ask")), TOKEN), Some(QUESTION));
        assert_eq!(route(&req("POST", &format!("/hook/{TOKEN}/permission")), TOKEN), Some(PERMISSION));
        assert_eq!(route(&req("POST", &format!("/hook/{TOKEN}/ask?x=1")), TOKEN), Some(QUESTION));
        assert_eq!(route(&req("GET", &format!("/hook/{TOKEN}/ask")), TOKEN), None);
        assert_eq!(route(&req("POST", "/hook/wrong/ask"), TOKEN), None);
        assert_eq!(route(&req("POST", &format!("/hook/{TOKEN}/other")), TOKEN), None);
        assert_eq!(route(&req("POST", &format!("/hook/{TOKEN}/ask/more")), TOKEN), None);
        assert_eq!(route(&req("POST", &format!("/hook/{TOKEN}")), TOKEN), None);
        assert_eq!(route(&req("POST", "/"), TOKEN), None);
        assert_eq!(route(&req("POST", &format!("/x/{TOKEN}/ask")), TOKEN), None);
    }

    #[test]
    fn tokens_are_32_hex_characters_and_differ() {
        let (a, b) = (random_token(), random_token());
        assert!(valid_token(&a) && valid_token(&b));
        assert_ne!(a, b);
        assert!(!valid_token("short") && !valid_token(&"g".repeat(32)));
    }

    #[test]
    fn a_closed_connection_is_noticed_and_an_open_one_is_not() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        server.set_nonblocking(true).unwrap();
        assert!(!peer_closed(&server));
        drop(client);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !peer_closed(&server) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(peer_closed(&server));
    }

    #[test]
    fn waiting_returns_the_answer_and_gives_up_on_a_hang_up() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(serde_json::json!({ "ok": true })).unwrap();
        assert!(matches!(wait(&server, &rx, Duration::from_secs(5)), Waited::Reply(v) if v["ok"] == true));
        assert!(matches!(wait(&server, &rx, Duration::from_millis(1)), Waited::TimedOut | Waited::Closed));
        let (_keep, rx) = mpsc::channel::<Value>();
        drop(client);
        assert!(matches!(wait(&server, &rx, Duration::from_secs(5)), Waited::Closed));
    }
}
