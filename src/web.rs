//! Small dependency-free HTTP surface for the local playable board.
use crate::chess_move::Move;
use crate::hash::repetition_key;
use crate::movegen::{in_check, legal_moves};
use crate::search::Searcher;
use crate::{Color, Position};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const INDEX: &str = include_str!("../web/index.html");
const STYLE: &str = include_str!("../web/style.css");
const APP: &str = include_str!("../web/app.js");
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024;
const MAX_MOVE_BYTES: usize = 5;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_DEPTH: u8 = 1;
const MAX_DEPTH: u8 = 6;

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Response {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    fn text(status: u16, body: &str, content_type: &'static str) -> Self {
        Self {
            status,
            content_type,
            body: body.to_owned(),
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self::json(status, format!(r#"{{"error":"{}"}}"#, json_escape(message)))
    }
}

/// A single local game where the browser is always White and the engine is Black.
pub struct GameSession {
    position: Position,
    history: Vec<u64>,
    moves: Vec<String>,
    depth: u8,
}

impl GameSession {
    pub fn new(depth: u8) -> Self {
        let position = Position::startpos();
        Self {
            history: vec![repetition_key(&position)],
            position,
            moves: Vec::new(),
            depth: clamp_depth(depth),
        }
    }

    pub fn reset(&mut self, depth: Option<u8>) {
        if let Some(depth) = depth {
            self.depth = clamp_depth(depth);
        }
        self.position = Position::startpos();
        self.history = vec![repetition_key(&self.position)];
        self.moves.clear();
    }

    pub fn state_json(&mut self) -> String {
        let legal =
            if self.position.side_to_move() == Color::White && self.terminal_status().is_none() {
                legal_moves(&mut self.position)
                    .into_iter()
                    .map(|mv| mv.to_string())
                    .collect()
            } else {
                Vec::new()
            };
        let status = self.terminal_status().unwrap_or_else(|| {
            if self.position.side_to_move() == Color::White {
                "Your move".to_owned()
            } else {
                "Engine to move".to_owned()
            }
        });
        format!(
            r#"{{"fen":"{}","legalMoves":[{}],"moves":[{}],"status":"{}","terminal":{},"depth":{},"sideToMove":"{}","thinking":false}}"#,
            json_escape(&self.position.to_fen()),
            legal
                .into_iter()
                .map(|mv: String| format!(r#""{}""#, mv))
                .collect::<Vec<_>>()
                .join(","),
            self.moves
                .iter()
                .map(|mv| format!(r#""{}""#, mv))
                .collect::<Vec<_>>()
                .join(","),
            json_escape(&status),
            self.terminal_status().is_some(),
            self.depth,
            if self.position.side_to_move() == Color::White {
                "white"
            } else {
                "black"
            },
        )
    }

    pub fn play_human_move(&mut self, uci: &str) -> Result<(), String> {
        if self.terminal_status().is_some() {
            return Err("This game has already finished. Start a new game.".to_owned());
        }
        if self.position.side_to_move() != Color::White {
            return Err("Please wait for the engine's move.".to_owned());
        }
        let parsed = Move::from_uci(uci)
            .ok_or_else(|| "Move must be a UCI move such as e2e4.".to_owned())?;
        let mv = legal_moves(&mut self.position)
            .into_iter()
            .find(|candidate| *candidate == parsed)
            .ok_or_else(|| "That move is not legal in the current position.".to_owned())?;
        self.apply(mv);
        if self.terminal_status().is_none() {
            let history = &self.history[..self.history.len().saturating_sub(1)];
            let result =
                Searcher::new().search_with_history(&mut self.position, self.depth, None, history);
            if let Some(reply) = result.best_move {
                self.apply(reply);
            }
        }
        Ok(())
    }

    fn apply(&mut self, mv: Move) {
        self.position.make_move(mv);
        self.moves.push(mv.to_string());
        self.history.push(repetition_key(&self.position));
    }

    fn terminal_status(&mut self) -> Option<String> {
        let legal = legal_moves(&mut self.position);
        if legal.is_empty() {
            return Some(if in_check(&self.position, self.position.side_to_move()) {
                if self.position.side_to_move() == Color::White {
                    "Checkmate — Black wins."
                } else {
                    "Checkmate — White wins."
                }
                .to_owned()
            } else {
                "Draw — stalemate.".to_owned()
            });
        }
        if self.position.is_insufficient_material() {
            return Some("Draw — insufficient material.".to_owned());
        }
        if self.position.halfmove_clock() >= 100 {
            return Some("Draw — fifty-move rule.".to_owned());
        }
        let current = repetition_key(&self.position);
        if self.history.iter().filter(|key| **key == current).count() >= 3 {
            return Some("Draw — threefold repetition.".to_owned());
        }
        None
    }
}

pub fn route(session: &mut GameSession, request: &Request) -> Response {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => Response::text(200, INDEX, "text/html; charset=utf-8"),
        ("GET", "/style.css") => Response::text(200, STYLE, "text/css; charset=utf-8"),
        ("GET", "/app.js") => Response::text(200, APP, "application/javascript; charset=utf-8"),
        ("GET", "/api/state") => Response::json(200, session.state_json()),
        ("POST", "/api/new") => match optional_depth(&request.body) {
            Ok(depth) => {
                session.reset(depth);
                Response::json(200, session.state_json())
            }
            Err(message) => Response::error(400, &message),
        },
        ("POST", "/api/move") => match json_string_field(&request.body, "move") {
            Some(mv) => match session.play_human_move(&mv) {
                Ok(()) => Response::json(200, session.state_json()),
                Err(message) => Response::error(400, &message),
            },
            None => Response::error(400, "Expected JSON with a string move field."),
        },
        ("GET", path) if path.starts_with("/api/") => Response::error(404, "Unknown API endpoint."),
        _ => Response::text(404, "Not found\n", "text/plain; charset=utf-8"),
    }
}

pub fn serve(address: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(address)?;
    eprintln!("Rookery web board listening at http://{address}");
    let session = Arc::new(Mutex::new(GameSession::new(3)));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let session = Arc::clone(&session);
                thread::spawn(move || handle_connection(stream, session));
            }
            Err(error) => eprintln!("web connection error: {error}"),
        }
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream, session: Arc<Mutex<GameSession>>) {
    let _ = stream.set_read_timeout(Some(CONNECTION_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));
    let response = match read_request(&mut stream) {
        Ok(request) => {
            let mut session = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            route(&mut session, &request)
        }
        Err(message) => Response::error(400, &message),
    };
    let _ = write_response(&mut stream, &response);
}

pub fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut header_bytes = 0;
    let first = read_header_line(stream, &mut header_bytes)?;
    let first =
        std::str::from_utf8(&first).map_err(|_| "Request header must be UTF-8.".to_owned())?;
    let mut parts = first
        .trim_end_matches(['\r', '\n'])
        .split_ascii_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "Malformed request line.".to_owned())?;
    let path = parts
        .next()
        .ok_or_else(|| "Malformed request line.".to_owned())?;
    let version = parts
        .next()
        .ok_or_else(|| "Malformed request line.".to_owned())?;
    if parts.next().is_some()
        || version != "HTTP/1.1"
        || !matches!(method, "GET" | "POST")
        || !path.starts_with('/')
        || path.contains('?')
        || path.contains("..")
    {
        return Err("Malformed request.".to_owned());
    }

    let mut content_length = 0usize;
    let mut saw_content_length = false;
    loop {
        let line = read_header_line(stream, &mut header_bytes)?;
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        let line = std::str::from_utf8(&line)
            .map_err(|_| "Request header must be UTF-8.".to_owned())?
            .trim_end_matches(['\r', '\n']);
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "Malformed request header.".to_owned())?;
        if name.eq_ignore_ascii_case("content-length") {
            if saw_content_length {
                return Err("Duplicate Content-Length.".to_owned());
            }
            saw_content_length = true;
            content_length = value
                .trim()
                .parse()
                .map_err(|_| "Invalid Content-Length.".to_owned())?;
            if content_length > MAX_BODY_BYTES {
                return Err("Request body is too large.".to_owned());
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("Transfer-Encoding is not supported.".to_owned());
        }
    }
    let mut body = vec![0; content_length];
    stream
        .read_exact(&mut body)
        .map_err(|_| "Incomplete request body.".to_owned())?;
    let body = String::from_utf8(body).map_err(|_| "Request body must be UTF-8.".to_owned())?;
    Ok(Request {
        method: method.to_owned(),
        path: path.to_owned(),
        body,
    })
}

fn read_header_line(stream: &mut TcpStream, total: &mut usize) -> Result<Vec<u8>, String> {
    let mut line = Vec::new();
    loop {
        if *total >= MAX_HEADER_BYTES {
            return Err("Request header is too large.".to_owned());
        }
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .map_err(|_| "Could not read request.".to_owned())?;
        *total += 1;
        line.push(byte[0]);
        if byte[0] == b'\n' {
            return Ok(line);
        }
    }
}

fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
        response.body
    )
}

fn optional_depth(body: &str) -> Result<Option<u8>, String> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    let value = json_number_field(body, "depth")
        .ok_or_else(|| "Expected JSON with a numeric depth field.".to_owned())?;
    let depth: u8 = value
        .parse()
        .map_err(|_| "Depth must be a whole number.".to_owned())?;
    if !(MIN_DEPTH..=MAX_DEPTH).contains(&depth) {
        return Err("Depth must be between 1 and 6.".to_owned());
    }
    Ok(Some(depth))
}

fn clamp_depth(depth: u8) -> u8 {
    depth.clamp(MIN_DEPTH, MAX_DEPTH)
}

fn json_string_field(body: &str, name: &str) -> Option<String> {
    let mut parser = JsonParser::new(body);
    parser.whitespace();
    parser.byte(b'{')?;
    parser.whitespace();
    let key = parser.string(name.len())?;
    if key != name {
        return None;
    }
    parser.whitespace();
    parser.byte(b':')?;
    parser.whitespace();
    let value = parser.string(MAX_MOVE_BYTES)?;
    parser.whitespace();
    parser.byte(b'}')?;
    parser.whitespace();
    if !parser.at_end() || !value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn json_number_field<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let mut parser = JsonParser::new(body);
    parser.whitespace();
    parser.byte(b'{')?;
    parser.whitespace();
    if parser.string(name.len())? != name {
        return None;
    }
    parser.whitespace();
    parser.byte(b':')?;
    parser.whitespace();
    let start = parser.cursor;
    while parser.peek().is_some_and(|byte| byte.is_ascii_digit()) {
        parser.cursor += 1;
    }
    if parser.cursor == start {
        return None;
    }
    let value = &body[start..parser.cursor];
    parser.whitespace();
    parser.byte(b'}')?;
    parser.whitespace();
    parser.at_end().then_some(value)
}

struct JsonParser<'a> {
    input: &'a [u8],
    cursor: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            cursor: 0,
        }
    }
    fn whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.cursor += 1;
        }
    }
    fn byte(&mut self, expected: u8) -> Option<()> {
        (self.peek()? == expected).then(|| self.cursor += 1)
    }
    fn string(&mut self, max_bytes: usize) -> Option<&'a str> {
        self.byte(b'"')?;
        let start = self.cursor;
        while let Some(byte) = self.peek() {
            if byte == b'"' {
                let value = std::str::from_utf8(&self.input[start..self.cursor]).ok()?;
                self.cursor += 1;
                return (value.len() <= max_bytes
                    && !value.bytes().any(|byte| byte < 0x20 || byte == b'\\'))
                .then_some(value);
            }
            if byte < 0x20 || byte == b'\\' {
                return None;
            }
            self.cursor += 1;
        }
        None
    }
    fn peek(&self) -> Option<u8> {
        self.input.get(self.cursor).copied()
    }
    fn at_end(&self) -> bool {
        self.cursor == self.input.len()
    }
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_contains_starting_legal_moves() {
        let mut session = GameSession::new(3);
        let state = session.state_json();
        assert!(state.contains("\"fen\":\"rnbqkbnr/pppppppp"));
        assert!(state.contains("\"e2e4\""));
        assert!(state.contains("\"status\":\"Your move\""));
    }

    #[test]
    fn legal_human_move_gets_engine_reply() {
        let mut session = GameSession::new(1);
        session.play_human_move("e2e4").unwrap();
        let state = session.state_json();
        assert!(state.contains("\"moves\":[\"e2e4\","));
        assert!(state.contains("\"sideToMove\":\"white\""));
    }

    #[test]
    fn routes_reject_bad_moves_and_unknown_assets() {
        let mut session = GameSession::new(1);
        let bad = route(
            &mut session,
            &Request {
                method: "POST".into(),
                path: "/api/move".into(),
                body: r#"{"move":"e2e5"}"#.into(),
            },
        );
        assert_eq!(bad.status, 400);
        let hidden = route(
            &mut session,
            &Request {
                method: "GET".into(),
                path: "/Cargo.toml".into(),
                body: String::new(),
            },
        );
        assert_eq!(hidden.status, 404);
    }

    #[test]
    fn depth_input_is_bounded() {
        assert_eq!(optional_depth(r#"{"depth":6}"#), Ok(Some(6)));
        assert!(optional_depth(r#"{"depth":7}"#).is_err());
        assert!(optional_depth(r#"{"depth":"3"}"#).is_err());
        assert!(optional_depth(r#"{"depth":3} trailing"#).is_err());
    }

    #[test]
    fn move_json_requires_one_complete_object_and_does_not_mutate_on_failure() {
        for body in [
            r#"{"move":"e2e4""#,
            r#"{"move":"e2e4"} trailing"#,
            r#"{"move":"e2e4","other":"value"}"#,
            r#"{"move":"e2e4","move":"a2a3"}"#,
            r#"{"move":"e2\\u00654"}"#,
            r#"{"move":"e2e4"}}"#,
        ] {
            let mut session = GameSession::new(1);
            let response = route(
                &mut session,
                &Request {
                    method: "POST".into(),
                    path: "/api/move".into(),
                    body: body.into(),
                },
            );
            assert_eq!(response.status, 400, "accepted malformed body: {body}");
            assert!(session.moves.is_empty(), "mutated session for: {body}");
        }
        assert_eq!(
            json_string_field(r#" { "move" : "E2E4" } "#, "move"),
            Some("e2e4".into())
        );
    }

    #[test]
    fn oversized_header_line_is_rejected_before_a_body_is_allocated() {
        use std::io::{Read, Write};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(address).unwrap();
        let (stream, _) = listener.accept().unwrap();
        let session = Arc::new(Mutex::new(GameSession::new(1)));
        let worker = thread::spawn(move || handle_connection(stream, session));
        let prefix = b"GET /api/state HTTP/1.1 ";
        client.write_all(prefix).unwrap();
        client
            .write_all(&vec![b'x'; MAX_HEADER_BYTES - prefix.len()])
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        worker.join().unwrap();
        assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert!(response.contains("Request header is too large."));
    }

    #[test]
    fn slow_client_does_not_block_another_connection() {
        use std::io::{Read, Write};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let slow_client = TcpStream::connect(address).unwrap();
        let (slow_stream, _) = listener.accept().unwrap();
        let session = Arc::new(Mutex::new(GameSession::new(1)));
        let slow_session = Arc::clone(&session);
        thread::spawn(move || handle_connection(slow_stream, slow_session));

        let mut fast_client = TcpStream::connect(address).unwrap();
        let (fast_stream, _) = listener.accept().unwrap();
        let fast_session = Arc::clone(&session);
        let worker = thread::spawn(move || handle_connection(fast_stream, fast_session));
        fast_client
            .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        fast_client.read_to_string(&mut response).unwrap();
        worker.join().unwrap();
        drop(slow_client);
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("\"fen\""));
    }
}
