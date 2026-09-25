use rookery::Position;

const EMPTY_BOARD: &str = "8/8/8/8/8/8/8/8";

#[test]
fn rejects_malformed_fen_fields() {
    let cases = [
        (
            "wrong field count",
            "8/8/8/8/8/8/8/8 w - - 0",
            "FEN must contain six fields",
        ),
        (
            "too few board ranks",
            "8/8/8/8/8/8/8 w - - 0 1",
            "FEN board must contain eight ranks",
        ),
        (
            "too many board ranks",
            "8/8/8/8/8/8/8/8/8 w - - 0 1",
            "FEN board must contain eight ranks",
        ),
        (
            "zero empty-square count",
            "80/8/8/8/8/8/8/8 w - - 0 1",
            "invalid empty-square count",
        ),
        (
            "oversized empty-square count",
            "9/8/8/8/8/8/8/8 w - - 0 1",
            "invalid empty-square count",
        ),
        (
            "short rank",
            "7/8/8/8/8/8/8/8 w - - 0 1",
            "rank does not contain eight squares",
        ),
        (
            "long rank",
            "8p/8/8/8/8/8/8/8 w - - 0 1",
            "rank has too many squares",
        ),
        (
            "unknown piece",
            "x7/8/8/8/8/8/8/8 w - - 0 1",
            "invalid FEN piece",
        ),
        (
            "invalid active color",
            "8/8/8/8/8/8/8/8 white - - 0 1",
            "invalid active color",
        ),
        (
            "invalid castling right",
            "8/8/8/8/8/8/8/8 w KA - 0 1",
            "invalid castling rights",
        ),
        (
            "duplicate castling right",
            "8/8/8/8/8/8/8/8 w KK - 0 1",
            "duplicate castling right",
        ),
        (
            "invalid en-passant square",
            "8/8/8/8/8/8/8/8 w - i6 0 1",
            "invalid en-passant square",
        ),
        (
            "en-passant rank for white",
            "8/8/8/8/8/8/8/8 w - e3 0 1",
            "en-passant square is on the wrong rank for the active color",
        ),
        (
            "en-passant rank for black",
            "8/8/8/8/8/8/8/8 b - e6 0 1",
            "en-passant square is on the wrong rank for the active color",
        ),
        (
            "negative halfmove clock",
            "8/8/8/8/8/8/8/8 w - - -1 1",
            "invalid halfmove clock",
        ),
        (
            "overflowing halfmove clock",
            "8/8/8/8/8/8/8/8 w - - 65536 1",
            "invalid halfmove clock",
        ),
        (
            "zero fullmove number",
            "8/8/8/8/8/8/8/8 w - - 0 0",
            "fullmove number must be positive",
        ),
        (
            "invalid fullmove number",
            "8/8/8/8/8/8/8/8 w - - 0 one",
            "invalid fullmove number",
        ),
    ];

    for (name, fen, expected_error) in cases {
        assert_eq!(
            Position::from_fen(fen).unwrap_err(),
            expected_error,
            "{name}: {fen}"
        );
    }
}

#[test]
fn accepts_position_invariants_outside_the_parser_contract() {
    // The parser validates FEN field shape, but deliberately does not require
    // kings, validate castling pieces, or prove an en-passant target is reachable.
    for fen in [
        format!("{EMPTY_BOARD} w - - 0 1"),
        format!("{EMPTY_BOARD} w KQkq - 0 1"),
        format!("{EMPTY_BOARD} w - e6 0 1"),
    ] {
        let position = Position::from_fen(&fen).unwrap();
        assert_eq!(position.to_fen(), fen);
    }
}
