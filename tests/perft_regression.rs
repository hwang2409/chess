use rookery::{Position, movegen::perft};

fn assert_perft_counts(fen: &str, expected_counts: &[(u8, u64)]) {
    let mut position = Position::from_fen(fen).unwrap();
    let initial_position = position.clone();

    for &(depth, expected) in expected_counts {
        assert_eq!(
            perft(&mut position, depth),
            expected,
            "FEN: {fen}, depth: {depth}"
        );
        assert_eq!(
            position, initial_position,
            "position changed: FEN: {fen}, depth: {depth}"
        );
    }
}

// FENs and reference counts are Chess Programming Wiki perft test positions 3 and 4.
const STANDARD_POSITIONS: [(&str, &[(u8, u64)]); 2] = [
    (
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        &[(1, 14), (2, 191), (3, 2_812)],
    ),
    (
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        &[(1, 6), (2, 264), (3, 9_467)],
    ),
];

#[test]
fn chess_programming_wiki_perft_regressions() {
    for (fen, expected_counts) in STANDARD_POSITIONS {
        assert_perft_counts(fen, expected_counts);
    }
}

#[test]
fn perft_rejects_moves_that_break_an_absolute_pin() {
    // The white rook on e2 is pinned to its king by the black rook on e8.
    // Its only legal rook moves remain on the e-file.
    assert_perft_counts("k3r3/8/8/8/8/8/4R3/4K3 w - - 0 1", &[(1, 10), (2, 120)]);
}

#[test]
fn perft_rejects_en_passant_that_exposes_a_horizontal_rook_check() {
    // Chess Programming Wiki's en-passant legality position has a nominal
    // c4xd3 capture. Taking it would clear c4 and d4, exposing the rook on
    // a4 to the black king on g4, so it must not be counted as a legal move.
    assert_perft_counts(
        "8/6bb/8/8/R1pP2k1/4P3/P7/K7 b - d3 0 1",
        &[(1, 21), (2, 206)],
    );
}

#[test]
fn perft_enforces_castling_transit_and_available_rights() {
    // The bishop on b5 attacks f1: white may castle queenside but not through
    // f1 kingside. Both rights are otherwise present and both rooks exist.
    assert_perft_counts("4k3/8/8/1b6/8/8/8/R3K2R w KQ - 0 1", &[(1, 23), (2, 267)]);

    // With no attacked transit squares, the same white arrangement permits
    // both castling moves; black's rights are retained for the next ply.
    assert_perft_counts("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", &[(1, 26), (2, 568)]);
}
