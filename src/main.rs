use std::collections::VecDeque;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rookery::Position;
use rookery::chess_move::Move;
use rookery::hash::repetition_key;
use rookery::movegen::{legal_moves, perft_divide};
use rookery::search::{SearchResult, Searcher};

struct ActiveSearch {
    cancelled: Arc<AtomicBool>,
    result: Receiver<SearchResult>,
    worker: JoinHandle<()>,
}

fn main() {
    let (commands, command_receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if commands.send(line).is_err() {
                break;
            }
        }
    });

    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut position = Position::startpos();
    let mut history = vec![repetition_key(&position)];
    let mut active: Option<ActiveSearch> = None;
    let mut deferred = VecDeque::new();
    let mut quitting = false;

    loop {
        if let Some(search) = active.as_ref()
            && let Ok(result) = search.result.try_recv()
        {
            let search = active.take().unwrap();
            search.worker.join().unwrap();
            if quitting {
                break;
            }
            write_search_result(&mut stdout, result);
            stdout.flush().unwrap();
            continue;
        }

        let command = if active.is_none() {
            deferred
                .pop_front()
                .or_else(|| command_receiver.recv().ok())
        } else {
            match command_receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(search) = active.as_ref() {
                        search.cancelled.store(true, Ordering::Relaxed);
                    }
                    quitting = true;
                    None
                }
            }
        };
        let Some(line) = command else {
            if active.is_none() {
                break;
            }
            continue;
        };

        let mut words = line.split_whitespace();
        match words.next().unwrap_or("") {
            "uci" => {
                writeln!(stdout, "id name Rookery").unwrap();
                writeln!(stdout, "id author Rookery contributors").unwrap();
                writeln!(stdout, "uciok").unwrap();
            }
            "isready" => writeln!(stdout, "readyok").unwrap(),
            "ucinewgame" if active.is_none() => {
                position = Position::startpos();
                history = vec![repetition_key(&position)];
            }
            "position" if active.is_none() => match parse_position(&line) {
                Ok((next, next_history)) => {
                    position = next;
                    history = next_history;
                }
                Err(error) => writeln!(stdout, "info string invalid position: {error}").unwrap(),
            },
            "go" if active.is_none() => {
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
                    active = Some(start_search(&position, &history, &args));
                }
            }
            "stop" => {
                if let Some(search) = active.as_ref() {
                    search.cancelled.store(true, Ordering::Relaxed);
                }
            }
            "quit" => {
                if let Some(search) = active.as_ref() {
                    search.cancelled.store(true, Ordering::Relaxed);
                    quitting = true;
                } else {
                    break;
                }
            }
            "ucinewgame" | "position" | "go" => deferred.push_back(line),
            "" => {}
            _ => {}
        }
        stdout.flush().unwrap();
    }
}

fn start_search(position: &Position, history: &[u64], args: &[&str]) -> ActiveSearch {
    let depth = value_after(args, "depth")
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(8)
        .clamp(1, 32);
    let time = search_time(args, position.side_to_move());
    let mut search_position = position.clone();
    let search_history = history[..history.len().saturating_sub(1)].to_vec();
    let cancelled = Arc::new(AtomicBool::new(false));
    let search_cancelled = Arc::clone(&cancelled);
    let (result_sender, result) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let result = Searcher::new().search_with_history_cancelled(
            &mut search_position,
            depth,
            time,
            &search_history,
            Some(search_cancelled),
        );
        let _ = result_sender.send(result);
    });
    ActiveSearch {
        cancelled,
        result,
        worker,
    }
}

fn write_search_result(stdout: &mut impl Write, result: SearchResult) {
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
