use crate::{Color, PieceKind, Position, Square, attacks};
use std::sync::OnceLock;

struct Keys {
    piece: [[u64; 64]; 12],
    side: u64,
    castling: [u64; 16],
    ep_file: [u64; 8],
}
static KEYS: OnceLock<Keys> = OnceLock::new();

fn keys() -> &'static Keys {
    KEYS.get_or_init(|| {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        };
        let mut piece = [[0; 64]; 12];
        for row in &mut piece {
            for x in row {
                *x = next();
            }
        }
        let side = next();
        let mut castling = [0; 16];
        for x in &mut castling {
            *x = next();
        }
        let mut ep_file = [0; 8];
        for x in &mut ep_file {
            *x = next();
        }
        Keys {
            piece,
            side,
            castling,
            ep_file,
        }
    })
}

/// Position identity for repetition: board, side, castling, and an en-passant
/// file only when the side to move can legally capture en passant.
pub fn repetition_key(p: &Position) -> u64 {
    board_key(p)
}

pub fn hash_position(p: &Position) -> u64 {
    board_key(p) ^ u64::from(p.halfmove_clock()).wrapping_mul(0x517c_c1b7_2722_0a95)
}

fn board_key(p: &Position) -> u64 {
    let k = keys();
    let mut hash = k.castling[p.castling_rights() as usize];
    for color in [Color::White, Color::Black] {
        for kind in [
            PieceKind::Pawn,
            PieceKind::Knight,
            PieceKind::Bishop,
            PieceKind::Rook,
            PieceKind::Queen,
            PieceKind::King,
        ] {
            let mut bb = p.pieces(color, kind);
            let piece_idx = color as usize * 6 + kind as usize;
            while bb != 0 {
                let sq = bb.trailing_zeros() as usize;
                hash ^= k.piece[piece_idx][sq];
                bb &= bb - 1;
            }
        }
    }
    if p.side_to_move() == Color::Black {
        hash ^= k.side;
    }
    if let Some(ep) = legal_en_passant_square(p) {
        hash ^= k.ep_file[ep.file() as usize];
    }
    hash
}

/// Returns the en-passant target only when at least one capture is legal.
///
/// This intentionally models the capture on temporary bitboards instead of
/// calling move generation or mutating `Position`: hashing remains independent
/// of move generation and cannot disturb make/unmake state.
fn legal_en_passant_square(p: &Position) -> Option<Square> {
    let target = p.en_passant_square()?;
    let us = p.side_to_move();
    let them = us.opposite();
    if p.piece_at(target).is_some() {
        return None;
    }
    let captured = target.offset(0, if us == Color::White { -1 } else { 1 })?;
    if p.piece_at(captured)
        != Some(crate::Piece {
            color: them,
            kind: PieceKind::Pawn,
        })
    {
        return None;
    }

    let candidates = attacks::pawn(target, them) & p.pieces(us, PieceKind::Pawn);
    let king = p.king_square(us)?;
    let occupied = p.occupied();
    let enemy_pawns = p.pieces(them, PieceKind::Pawn) & !captured.bit();
    let mut remaining = candidates;
    while remaining != 0 {
        let from = Square::new(remaining.trailing_zeros() as u8).unwrap();
        remaining &= remaining - 1;
        let after = (occupied & !from.bit() & !captured.bit()) | target.bit();
        if !is_square_attacked(p, king, them, enemy_pawns, after) {
            return Some(target);
        }
    }
    None
}

/// Attack probe for an en-passant capture represented by `occupied` and the
/// enemy pawn set after its captured pawn has been removed.
fn is_square_attacked(p: &Position, square: Square, by: Color, pawns: u64, occupied: u64) -> bool {
    attacks::pawn(square, by.opposite()) & pawns != 0
        || attacks::knight(square) & p.pieces(by, PieceKind::Knight) != 0
        || attacks::king(square) & p.pieces(by, PieceKind::King) != 0
        || attacks::bishop(square, occupied)
            & (p.pieces(by, PieceKind::Bishop) | p.pieces(by, PieceKind::Queen))
            != 0
        || attacks::rook(square, occupied)
            & (p.pieces(by, PieceKind::Rook) | p.pieces(by, PieceKind::Queen))
            != 0
}

#[cfg(test)]
mod tests {
    use super::repetition_key;
    use crate::Position;
    use crate::movegen::legal_moves;

    fn position(fen: &str) -> Position {
        Position::from_fen(fen).unwrap()
    }

    #[test]
    fn repetition_ignores_en_passant_without_an_adjacent_pawn() {
        let with_target = position("4k3/8/8/3p4/8/8/8/4K3 w - d6 0 1");
        let without_target = position("4k3/8/8/3p4/8/8/8/4K3 w - - 0 1");
        assert_eq!(
            repetition_key(&with_target),
            repetition_key(&without_target)
        );
    }

    #[test]
    fn repetition_ignores_pseudo_legal_but_pinned_en_passant() {
        // c4xd3 would expose the rook on a4 to Black's king on g4.
        let with_target = position("8/6bb/8/8/R1pP2k1/4P3/P7/K7 b - d3 0 1");
        let without_target = position("8/6bb/8/8/R1pP2k1/4P3/P7/K7 b - - 0 1");
        let mut moves = with_target.clone();
        assert!(
            !legal_moves(&mut moves)
                .iter()
                .any(|mv| mv.to_string() == "c4d3")
        );
        assert_eq!(
            repetition_key(&with_target),
            repetition_key(&without_target)
        );
    }

    #[test]
    fn repetition_includes_a_legal_en_passant_file() {
        let with_target = position("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
        let without_target = position("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1");
        let mut moves = with_target.clone();
        assert!(
            legal_moves(&mut moves)
                .iter()
                .any(|mv| mv.to_string() == "e5d6")
        );
        assert_ne!(
            repetition_key(&with_target),
            repetition_key(&without_target)
        );
    }
}
