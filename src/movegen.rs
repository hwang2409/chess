use crate::chess_move::{Move, Promotion};
use crate::{Bitboard, Color, PieceKind, Position, Square, attacks};

const PROMOTIONS: [Promotion; 4] = [
    Promotion::Queen,
    Promotion::Rook,
    Promotion::Bishop,
    Promotion::Knight,
];

pub fn is_square_attacked(position: &Position, square: Square, by: Color) -> bool {
    let pawns = position.pieces(by, PieceKind::Pawn);
    if attacks::pawn(square, by.opposite()) & pawns != 0 {
        return true;
    }
    if attacks::knight(square) & position.pieces(by, PieceKind::Knight) != 0 {
        return true;
    }
    if attacks::king(square) & position.pieces(by, PieceKind::King) != 0 {
        return true;
    }
    let occupied = position.occupied();
    if attacks::bishop(square, occupied)
        & (position.pieces(by, PieceKind::Bishop) | position.pieces(by, PieceKind::Queen))
        != 0
    {
        return true;
    }
    attacks::rook(square, occupied)
        & (position.pieces(by, PieceKind::Rook) | position.pieces(by, PieceKind::Queen))
        != 0
}

pub fn in_check(position: &Position, color: Color) -> bool {
    position
        .king_square(color)
        .is_some_and(|king| is_square_attacked(position, king, color.opposite()))
}

pub fn pseudo_legal_moves(position: &Position) -> Vec<Move> {
    let us = position.side_to_move();
    let them = us.opposite();
    let own = position.occupied_by(us);
    let enemy = position.occupied_by(them);
    let occupied = own | enemy;
    let mut moves = Vec::with_capacity(64);

    for from_idx in 0..64 {
        let from = Square::new(from_idx).unwrap();
        let Some(piece) = position.piece_at(from).filter(|p| p.color == us) else {
            continue;
        };
        let targets: Bitboard = match piece.kind {
            PieceKind::Pawn => {
                let mut targets = 0;
                let step = if us == Color::White { 1 } else { -1 };
                if let Some(one) = from.offset(0, step).filter(|sq| occupied & sq.bit() == 0) {
                    targets |= one.bit();
                    let start_rank = if us == Color::White { 1 } else { 6 };
                    if from.rank() == start_rank
                        && let Some(two) = from
                            .offset(0, step * 2)
                            .filter(|sq| occupied & sq.bit() == 0)
                    {
                        targets |= two.bit();
                    }
                }
                targets
                    | (attacks::pawn(from, us)
                        & (enemy | position.en_passant_square().map_or(0, Square::bit)))
            }
            PieceKind::Knight => attacks::knight(from),
            PieceKind::Bishop => attacks::bishop(from, occupied),
            PieceKind::Rook => attacks::rook(from, occupied),
            PieceKind::Queen => attacks::queen(from, occupied),
            PieceKind::King => attacks::king(from),
        } & !own;

        let mut remaining = targets;
        while remaining != 0 {
            let to = Square::new(remaining.trailing_zeros() as u8).unwrap();
            remaining &= remaining - 1;
            if position
                .piece_at(to)
                .is_some_and(|p| p.kind == PieceKind::King)
            {
                continue;
            }
            if piece.kind == PieceKind::Pawn && (to.rank() == 0 || to.rank() == 7) {
                for promotion in PROMOTIONS {
                    moves.push(Move::new(from, to, Some(promotion)));
                }
            } else {
                moves.push(Move::new(from, to, None));
            }
        }
    }

    let (rank, ks_right, qs_right, king_start) = if us == Color::White {
        (0, 1, 2, "e1")
    } else {
        (7, 4, 8, "e8")
    };
    let start = Square::from_name(king_start).unwrap();
    if position
        .piece_at(start)
        .is_some_and(|p| p.kind == PieceKind::King)
        && !is_square_attacked(position, start, them)
    {
        if position.castling_rights() & ks_right != 0 {
            let f = Square::from_coords(5, rank).unwrap();
            let g = Square::from_coords(6, rank).unwrap();
            let rook = Square::from_coords(7, rank).unwrap();
            if position
                .piece_at(rook)
                .is_some_and(|p| p.color == us && p.kind == PieceKind::Rook)
                && position.occupied() & (f.bit() | g.bit()) == 0
                && !is_square_attacked(position, f, them)
                && !is_square_attacked(position, g, them)
            {
                moves.push(Move::new(start, g, None));
            }
        }
        if position.castling_rights() & qs_right != 0 {
            let b = Square::from_coords(1, rank).unwrap();
            let c = Square::from_coords(2, rank).unwrap();
            let d = Square::from_coords(3, rank).unwrap();
            let rook = Square::from_coords(0, rank).unwrap();
            if position
                .piece_at(rook)
                .is_some_and(|p| p.color == us && p.kind == PieceKind::Rook)
                && position.occupied() & (b.bit() | c.bit() | d.bit()) == 0
                && !is_square_attacked(position, d, them)
                && !is_square_attacked(position, c, them)
            {
                moves.push(Move::new(start, c, None));
            }
        }
    }
    moves
}

/// Returns whether the side to move has any legal move, stopping at the first one.
pub fn has_legal_move(position: &mut Position) -> bool {
    let us = position.side_to_move();
    for mv in pseudo_legal_moves(position) {
        let undo = position.make_move(mv);
        let valid = !in_check(position, us);
        position.unmake_move(mv, undo);
        if valid {
            return true;
        }
    }
    false
}

pub fn legal_moves(position: &mut Position) -> Vec<Move> {
    let us = position.side_to_move();
    let mut legal = Vec::new();
    for mv in pseudo_legal_moves(position) {
        let undo = position.make_move(mv);
        let valid = !in_check(position, us);
        position.unmake_move(mv, undo);
        if valid {
            legal.push(mv);
        }
    }
    legal
}

pub fn perft(position: &mut Position, depth: u8) -> u64 {
    if depth == 0 {
        return 1;
    }
    let moves = legal_moves(position);
    if depth == 1 {
        return moves.len() as u64;
    }
    let mut nodes = 0;
    for mv in moves {
        let undo = position.make_move(mv);
        nodes += perft(position, depth - 1);
        position.unmake_move(mv, undo);
    }
    nodes
}

pub fn perft_divide(position: &mut Position, depth: u8) -> Vec<(Move, u64)> {
    if depth == 0 {
        return Vec::new();
    }
    legal_moves(position)
        .into_iter()
        .map(|mv| {
            let undo = position.make_move(mv);
            let nodes = perft(position, depth - 1);
            position.unmake_move(mv, undo);
            (mv, nodes)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{has_legal_move, legal_moves, perft};
    use crate::Position;

    #[test]
    fn legal_move_probe_matches_generation_and_preserves_position() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1",
            "7k/6Q1/6K1/8/8/8/8/8 b - - 0 1",
        ] {
            let mut p = Position::from_fen(fen).unwrap();
            let original = p.clone();
            assert_eq!(has_legal_move(&mut p), !legal_moves(&mut p).is_empty());
            assert_eq!(p, original);
        }
    }

    #[test]
    fn start_position_perft() {
        let mut p = Position::startpos();
        for (depth, expected) in [(1, 20), (2, 400), (3, 8_902), (4, 197_281)] {
            assert_eq!(perft(&mut p, depth), expected, "depth {depth}");
            assert_eq!(p, Position::startpos(), "position changed at depth {depth}");
        }
    }

    #[test]
    fn kiwipete_perft() {
        let mut p = Position::from_fen(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        )
        .unwrap();
        assert_eq!(perft(&mut p, 1), 48);
        assert_eq!(perft(&mut p, 2), 2_039);
        assert_eq!(perft(&mut p, 3), 97_862);
    }

    #[test]
    fn promotion_en_passant_and_castling_legal_moves() {
        let mut p = Position::from_fen("4k3/P7/8/3pP3/8/8/8/4K2R w K d6 0 1").unwrap();
        let moves = legal_moves(&mut p);
        for suffix in ["q", "r", "b", "n"] {
            assert!(
                moves
                    .iter()
                    .any(|m| m.to_string() == format!("a7a8{suffix}"))
            );
        }
        assert!(moves.iter().any(|m| m.to_string() == "e5d6"));
        assert!(moves.iter().any(|m| m.to_string() == "e1g1"));
    }
}
