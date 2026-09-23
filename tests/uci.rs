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
