use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

fn start_engine() -> (Child, ChildStdin, Receiver<String>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rookery"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (lines, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    (child, stdin, receiver)
}

fn receive_through_bestmove(receiver: &Receiver<String>, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = receiver
            .recv_timeout(remaining)
            .expect("timed out waiting for bestmove");
        output.push_str(&line);
        output.push('\n');
        if line.starts_with("bestmove ") {
            return output;
        }
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "engine exited unsuccessfully: {status}");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "engine did not exit within {} ms",
            timeout.as_millis()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn quit_and_wait(mut child: Child, mut stdin: ChildStdin) {
    stdin.write_all(b"quit\n").unwrap();
    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(1));
}

fn synchronize_active_search(stdin: &mut ChildStdin, receiver: &Receiver<String>) -> String {
    stdin.write_all(b"isready\n").unwrap();
    let output = receive_until(receiver, Duration::from_secs(1), |line| line == "readyok");
    assert!(
        !output.lines().any(|line| line.starts_with("bestmove ")),
        "search completed before synchronization: {output}"
    );
    output
}

fn receive_through_bestmoves(
    receiver: &Receiver<String>,
    count: usize,
    timeout: Duration,
) -> String {
    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    let mut received = 0;
    while received < count {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = receiver
            .recv_timeout(remaining)
            .expect("timed out waiting for bestmove");
        if line.starts_with("bestmove ") {
            received += 1;
        }
        output.push_str(&line);
        output.push('\n');
    }
    output
}

#[test]
fn uci_handshake_and_search() {
    let (child, mut stdin, receiver) = start_engine();
    stdin
        .write_all(b"uci\nisready\nposition startpos\ngo depth 2\n")
        .unwrap();
    let text = receive_through_bestmove(&receiver, Duration::from_secs(2));
    assert!(text.contains("uciok"), "{text}");
    assert!(text.contains("readyok"), "{text}");
    assert!(text.contains("info depth 2"), "{text}");
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("bestmove "))
            .count(),
        1
    );
    quit_and_wait(child, stdin);
}

#[test]
fn uci_stop_interrupts_an_active_search_and_returns_one_bestmove() {
    let (child, mut stdin, receiver) = start_engine();
    stdin
        .write_all(b"uci\nposition startpos\ngo depth 32\n")
        .unwrap();
    let handshake = synchronize_active_search(&mut stdin, &receiver);
    assert!(handshake.contains("uciok"), "{handshake}");

    let stopped_at = Instant::now();
    stdin.write_all(b"stop\n").unwrap();
    let text = receive_through_bestmove(&receiver, Duration::from_secs(2));
    assert!(
        stopped_at.elapsed() < Duration::from_secs(2),
        "stop response was not timely: {text}"
    );
    assert!(
        text.lines().any(|line| line.starts_with("info depth ")),
        "{text}"
    );
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("bestmove "))
            .count(),
        1
    );
    assert!(
        text.lines()
            .all(|line| { line.starts_with("info depth ") || line.starts_with("bestmove ") }),
        "unexpected protocol output: {text}"
    );
    quit_and_wait(child, stdin);
}

#[test]
fn uci_defers_position_and_go_following_stop_until_search_finishes() {
    let (child, mut stdin, receiver) = start_engine();
    stdin
        .write_all(b"uci\nposition startpos\ngo depth 32\n")
        .unwrap();
    let handshake = synchronize_active_search(&mut stdin, &receiver);
    assert!(handshake.contains("uciok"), "{handshake}");

    stdin
        .write_all(b"stop\nposition startpos moves e2e4\ngo depth 1\n")
        .unwrap();
    let text = receive_through_bestmoves(&receiver, 2, Duration::from_secs(2));
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("bestmove "))
            .count(),
        2,
        "{text}"
    );
    assert!(text.contains("info depth 1"), "{text}");
    assert!(text.contains("bestmove b8c6"), "{text}");
    quit_and_wait(child, stdin);
}

#[test]
fn uci_queued_quit_suppresses_a_completed_search_response() {
    let (mut child, mut stdin, receiver) = start_engine();
    stdin
        .write_all(b"position startpos\ngo depth 32\n")
        .unwrap();

    // `readyok` is emitted by the command loop while the search worker is
    // active. Seeing it before any search response synchronizes this test with
    // an active search rather than relying on an unsynchronized `go`/`quit`
    // input batch.
    synchronize_active_search(&mut stdin, &receiver);

    // `stop` makes the worker produce its final result, while queued `quit`
    // must win over that result and suppress all search output.
    stdin.write_all(b"stop\nquit\n").unwrap();
    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(1));
    assert!(
        receiver.recv_timeout(Duration::from_millis(100)).is_err(),
        "quit emitted a final search response"
    );
}

fn receive_until(
    receiver: &Receiver<String>,
    timeout: Duration,
    predicate: impl Fn(&str) -> bool,
) -> String {
    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = receiver
            .recv_timeout(remaining)
            .expect("timed out waiting for output");
        output.push_str(&line);
        output.push('\n');
        if predicate(&line) {
            return output;
        }
    }
}

#[test]
fn uci_position_history_detects_threefold_repetition_at_root() {
    let (child, mut stdin, receiver) = start_engine();
    stdin
        .write_all(b"position startpos moves g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8\ngo depth 2\n")
        .unwrap();
    let text = receive_through_bestmove(&receiver, Duration::from_secs(2));
    assert!(text.contains("info depth 0 score cp 0 nodes 0"), "{text}");
    assert!(text.contains("bestmove "), "{text}");
    quit_and_wait(child, stdin);
}

#[test]
fn uci_perft_reports_root_and_total() {
    let (child, mut stdin, receiver) = start_engine();
    stdin.write_all(b"position startpos\ngo perft 2\n").unwrap();
    let text = receive_through_bestmove(&receiver, Duration::from_secs(2));
    assert!(text.contains("nodes 400"));
    quit_and_wait(child, stdin);
}
