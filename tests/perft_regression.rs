use rookery::{Position, movegen::perft};

// FENs and reference counts are Chess Programming Wiki perft test positions 3 and 4.
const POSITIONS: [(&str, &[(u8, u64)]); 2] = [
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
    for (fen, expected_counts) in POSITIONS {
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
}
