use rookery::chess_move::Move;
use rookery::movegen::legal_moves;
use rookery::{Position, Square};

#[derive(Clone, Copy)]
struct StepRng(u64);

impl StepRng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn square(name: &str) -> Square {
    Square::from_name(name).unwrap()
}

fn play_and_unwind(mut position: Position, seed: u64, plies: usize, first_move: Option<Move>) {
    let original = position.clone();
    let mut rng = StepRng(seed);
    let mut history = Vec::new();

    for ply in 0..plies {
        let moves = legal_moves(&mut position);
        if moves.is_empty() {
            break;
        }
        let mv = if ply == 0 {
            first_move.unwrap_or_else(|| moves[(rng.next() as usize) % moves.len()])
        } else {
            moves[(rng.next() as usize) % moves.len()]
        };
        assert!(moves.contains(&mv), "chosen move must be legal");
        let undo = position.make_move(mv);
        history.push((mv, undo));
    }

    while let Some((mv, undo)) = history.pop() {
        position.unmake_move(mv, undo);
    }
    assert_eq!(position, original, "unmake must exactly restore Position");
}

fn matching_move(
    position: &mut Position,
    from: &str,
    to: &str,
    promotion: Option<rookery::chess_move::Promotion>,
) -> Move {
    let expected = Move::new(square(from), square(to), promotion);
    assert!(
        legal_moves(position).contains(&expected),
        "expected move {from}{to} must be legal"
    );
    expected
}

#[test]
fn random_legal_sequences_round_trip_from_start_position() {
    play_and_unwind(Position::startpos(), 0x5eed_cafe_d00d_beef, 160, None);
}

#[test]
fn random_sequences_round_trip_special_rule_positions() {
    // White can castle, capture en-passant, and promote; black has a rook and
    // king, so the position also exercises restoration of both sides' rights.
    let fen = "r3k2r/P2p4/8/3pP3/8/8/8/R3K2R w KQkq d6 17 42";
    let mut position = Position::from_fen(fen).unwrap();
    let en_passant = matching_move(&mut position, "e5", "d6", None);
    play_and_unwind(
        Position::from_fen(fen).unwrap(),
        0x1234_5678_9abc_def0,
        80,
        Some(en_passant),
    );

    let mut position = Position::from_fen(fen).unwrap();
    let castle = matching_move(&mut position, "e1", "g1", None);
    play_and_unwind(
        Position::from_fen(fen).unwrap(),
        0xfedc_ba98_7654_3210,
        80,
        Some(castle),
    );

    // The promotion move is independently selected so this special transition
    // is guaranteed to occur, rather than depending on a random rollout.
    let promotion_fen = "4k3/P7/8/8/8/8/8/4K3 w - - 23 19";
    let mut position = Position::from_fen(promotion_fen).unwrap();
    let promotion = matching_move(
        &mut position,
        "a7",
        "a8",
        Some(rookery::chess_move::Promotion::Queen),
    );
    play_and_unwind(
        Position::from_fen(promotion_fen).unwrap(),
        0x0ddc_0ffe_e15e_beef,
        32,
        Some(promotion),
    );
}
