use std::io::{self, BufRead, Write};
use std::time::Duration;

use rookery::Position;
use rookery::chess_move::Move;
use rookery::hash::repetition_key;
use rookery::movegen::{legal_moves, perft_divide};
use rookery::search::Searcher;

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut position = Position::startpos();
    let mut searcher = Searcher::new();
    let mut history = vec![repetition_key(&position)];
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let mut words = line.split_whitespace();
        match words.next().unwrap_or("") {
            "uci" => {
                writeln!(stdout, "id name Rookery").unwrap();
                writeln!(stdout, "id author Rookery contributors").unwrap();
                writeln!(stdout, "uciok").unwrap();
            }
            "isready" => writeln!(stdout, "readyok").unwrap(),
            "ucinewgame" => {
                position = Position::startpos();
                history = vec![repetition_key(&position)];
                searcher.clear();
            }
            "position" => match parse_position(&line) {
                Ok((next, next_history)) => {
                    position = next;
                    history = next_history;
                }
                Err(error) => writeln!(stdout, "info string invalid position: {error}").unwrap(),
            },
            "go" => {
                let args: Vec<_> = line.split_whitespace().skip(1).collect();
                if let Some(i) = args.iter().position(|arg| *arg == "perft") {
                    let depth = args
                        .get(i + 1)
                        .and_then(|s| s.parse::<u8>().ok())
                        .unwrap_or(1);
                    let divide = perft_divide(&mut position, depth);
                    for (mv, nodes) in &divide {
                        writeln!(stdout, "{mv}: {nodes}").unwrap();
                    }
                    let total: u64 = divide.iter().map(|(_, nodes)| nodes).sum();
                    writeln!(stdout, "nodes {total}").unwrap();
                    writeln!(stdout, "bestmove 0000").unwrap();
                } else {
                    let depth = value_after(&args, "depth")
                        .and_then(|v| v.parse::<u8>().ok())
                        .unwrap_or(8)
                        .clamp(1, 32);
                    let time = search_time(&args, position.side_to_move());
                    let result = searcher.search_with_history(
                        &mut position,
                        depth,
                        time,
                        &history[..history.len().saturating_sub(1)],
                    );
                    let score = if result.score.abs() > 29_000 {
                        let moves = (30_000 - result.score.abs() + 1) / 2;
                        format!("mate {}", if result.score < 0 { -moves } else { moves })
                    } else {
                        format!("cp {}", result.score)
                    };
                    writeln!(
                        stdout,
                        "info depth {} score {} nodes {}",
                        result.depth, score, result.nodes
                    )
                    .unwrap();
                    writeln!(
                        stdout,
                        "bestmove {}",
                        result
                            .best_move
                            .map_or("0000".to_string(), |m| m.to_string())
                    )
                    .unwrap();
                }
            }
            "stop" => {}
            "quit" => break,
            "" => {}
            _ => {}
        }
        stdout.flush().unwrap();
    }
}

fn value_after<'a>(args: &'a [&str], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| *arg == key)
        .and_then(|i| args.get(i + 1).copied())
}

fn search_time(args: &[&str], side: rookery::Color) -> Option<Duration> {
    if let Some(ms) = value_after(args, "movetime").and_then(|v| v.parse::<u64>().ok()) {
        return Some(Duration::from_millis(ms.max(1)));
    }
    let time_key = if side == rookery::Color::White {
        "wtime"
    } else {
        "btime"
    };
    let inc_key = if side == rookery::Color::White {
        "winc"
    } else {
        "binc"
    };
    let remaining = value_after(args, time_key)?.parse::<u64>().ok()?;
    let increment = value_after(args, inc_key)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    Some(Duration::from_millis(
        (remaining / 30 + increment * 3 / 4)
            .max(1)
            .min(remaining.max(1)),
    ))
}

fn parse_position(line: &str) -> Result<(Position, Vec<u64>), String> {
    let words: Vec<_> = line.split_whitespace().collect();
    if words.len() < 2 {
        return Err("missing startpos or fen".into());
    }
    let (mut position, mut index) = match words[1] {
        "startpos" => (Position::startpos(), 2),
        "fen" => {
            let move_index = words
                .iter()
                .position(|w| *w == "moves")
                .unwrap_or(words.len());
            if move_index < 8 {
                return Err("FEN position needs six fields".into());
            }
            (
                Position::from_fen(&words[2..move_index].join(" "))?,
                move_index,
            )
        }
        _ => return Err("expected startpos or fen".into()),
    };
    let mut history = vec![repetition_key(&position)];
    if words.get(index) == Some(&"moves") {
        index += 1;
    }
    for text in &words[index..] {
        let requested = Move::from_uci(text).ok_or_else(|| format!("invalid move {text}"))?;
        let actual = legal_moves(&mut position)
            .into_iter()
            .find(|mv| *mv == requested)
            .ok_or_else(|| format!("illegal move {text}"))?;
        position.make_move(actual);
        history.push(repetition_key(&position));
    }
    Ok((position, history))
}
