//! Small dependency-free HTTP surface for the local playable board.
use crate::chess_move::Move;
use crate::hash::repetition_key;
use crate::movegen::{in_check, legal_moves};
use crate::search::Searcher;
use crate::{Color, Position};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

const INDEX: &str = include_str!("../web/index.html");
const STYLE: &str = include_str!("../web/style.css");
const APP: &str = include_str!("../web/app.js");
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024;
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
    let mut session = GameSession::new(3);
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let response = match read_request(&mut stream) {
                    Ok(request) => route(&mut session, &request),
                    Err(message) => Response::error(400, &message),
                };
                let _ = write_response(&mut stream, &response);
            }
            Err(error) => eprintln!("web connection error: {error}"),
        }
    }
    Ok(())
}

pub fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader
        .read_line(&mut first)
        .map_err(|_| "Could not read request.".to_owned())?;
    if first.len() > MAX_HEADER_BYTES {
        return Err("Request header is too large.".to_owned());
    }
    let mut parts = first.split_whitespace();
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
    let mut bytes = first.len();
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|_| "Could not read request.".to_owned())?;
        bytes += line.len();
        if bytes > MAX_HEADER_BYTES {
            return Err("Request header is too large.".to_owned());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value
                .trim()
                .parse()
                .map_err(|_| "Invalid Content-Length.".to_owned())?;
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err("Request body is too large.".to_owned());
    }
    let mut body = vec![0; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|_| "Incomplete request body.".to_owned())?;
    let body = String::from_utf8(body).map_err(|_| "Request body must be UTF-8.".to_owned())?;
    Ok(Request {
        method: method.to_owned(),
        path: path.to_owned(),
        body,
    })
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
    let marker = format!(r#""{name}""#);
    let rest = body
        .trim()
        .strip_prefix('{')?
        .strip_suffix('}')?
        .split_once(&marker)?
        .1;
    let value = rest.trim_start().strip_prefix(':')?.trim_start();
    let text = value.strip_prefix('"')?.split('"').next()?;
    if text.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        Some(text.to_ascii_lowercase())
    } else {
        None
    }
}

fn json_number_field<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!(r#""{name}""#);
    let rest = body
        .trim()
        .strip_prefix('{')?
        .strip_suffix('}')?
        .split_once(&marker)?
        .1;
    let value = rest.trim_start().strip_prefix(':')?.trim();
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())).then_some(value)
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
    }
}
