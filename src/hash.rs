use crate::{Color, PieceKind, Position};
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

/// Position identity for repetition: board, side, castling, and en-passant state.
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
    if let Some(ep) = p.en_passant_square() {
        hash ^= k.ep_file[ep.file() as usize];
    }
    hash
}
