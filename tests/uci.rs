use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn uci_handshake_and_search() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rookery"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"uci\nisready\nposition startpos\ngo depth 2\nquit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("uciok"));
    assert!(text.contains("readyok"));
    assert!(text.contains("info depth 2"));
    assert!(text.contains("bestmove "));
}

#[test]
fn uci_position_history_detects_threefold_repetition_at_root() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rookery"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"position startpos moves g1f3 g8f6 f3g1 f6g8 g1f3 g8f6 f3g1 f6g8\ngo depth 2\nquit\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("info depth 0 score cp 0 nodes 0"), "{text}");
    assert!(text.contains("bestmove "), "{text}");
}

#[test]
fn uci_perft_reports_root_and_total() {
    let output = Command::new(env!("CARGO_BIN_EXE_rookery"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = output;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"position startpos\ngo perft 2\nquit\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("nodes 400"));
}
