use rookery::Position;
use rookery::hash::repetition_key;
use rookery::movegen::legal_moves;

fn position(fen: &str) -> Position {
    Position::from_fen(fen).unwrap()
}

fn has_legal_move(position: &mut Position, uci: &str) -> bool {
    legal_moves(position)
        .iter()
        .any(|candidate| candidate.to_string() == uci)
}

#[test]
fn black_legal_en_passant_candidates_change_repetition_identity() {
    // White has just advanced d2-d4. Either black pawn can legally capture d3.
    let with_target = position("4k3/8/8/8/2pPp3/8/8/4K3 b - d3 0 1");
    let without_target = position("4k3/8/8/8/2pPp3/8/8/4K3 b - - 0 1");
    let mut moves_position = with_target.clone();

    assert!(has_legal_move(&mut moves_position, "c4d3"));
    assert!(has_legal_move(&mut moves_position, "e4d3"));
    assert_ne!(
        repetition_key(&with_target),
        repetition_key(&without_target)
    );
}

#[test]
fn an_unpinned_en_passant_candidate_keeps_target_in_repetition_identity() {
    // c4xd3 would expose the rook on c1 to Black's king on c5, but e4xd3
    // leaves c4 in place and is legal.
    let with_target = position("8/8/8/2k5/2pPp3/8/8/2R1K3 b - d3 0 1");
    let without_target = position("8/8/8/2k5/2pPp3/8/8/2R1K3 b - - 0 1");
    let mut moves_position = with_target.clone();

    assert!(!has_legal_move(&mut moves_position, "c4d3"));
    assert!(has_legal_move(&mut moves_position, "e4d3"));
    assert_ne!(
        repetition_key(&with_target),
        repetition_key(&without_target)
    );
}

#[test]
fn legal_en_passant_hash_probe_does_not_mutate_position() {
    let position = position("4k3/8/8/8/2pPp3/8/8/4K3 b - d3 17 42");
    let original = position.clone();
    let original_fen = position.to_fen();
    let original_hash = position.zobrist_hash();

    let first_key = repetition_key(&position);
    let second_key = repetition_key(&position);

    assert_eq!(first_key, second_key);
    assert_eq!(position, original);
    assert_eq!(position.to_fen(), original_fen);
    assert_eq!(position.zobrist_hash(), original_hash);
}
