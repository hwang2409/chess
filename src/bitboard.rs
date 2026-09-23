use std::fmt;

pub type Bitboard = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    #[inline]
    pub const fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum PieceKind {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

/// Board square indexed a1=0, b1=1, ..., h8=63.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(transparent)]
pub struct Square(u8);

impl Square {
    pub const fn new(index: u8) -> Option<Self> {
        if index < 64 { Some(Self(index)) } else { None }
    }

    pub const fn index(self) -> u8 {
        self.0
    }
    pub const fn file(self) -> u8 {
        self.0 % 8
    }
    pub const fn rank(self) -> u8 {
        self.0 / 8
    }
    pub const fn bit(self) -> Bitboard {
        1u64 << self.0
    }

    pub fn from_coords(file: u8, rank: u8) -> Option<Self> {
        if file < 8 && rank < 8 {
            Some(Self(rank * 8 + file))
        } else {
            None
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let bytes = name.as_bytes();
        if bytes.len() != 2
            || !(b'a'..=b'h').contains(&bytes[0])
            || !(b'1'..=b'8').contains(&bytes[1])
        {
            return None;
        }
        Self::from_coords(bytes[0] - b'a', bytes[1] - b'1')
    }

    pub const fn offset(self, df: i8, dr: i8) -> Option<Self> {
        let file = self.file() as i8 + df;
        let rank = self.rank() as i8 + dr;
        if file >= 0 && file < 8 && rank >= 0 && rank < 8 {
            Some(Self((rank as u8) * 8 + file as u8))
        } else {
            None
        }
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            (b'a' + self.file()) as char,
            (b'1' + self.rank()) as char
        )
    }
}

pub mod attacks {
    use super::{Bitboard, Color, Square};

    const KNIGHT_STEPS: [(i8, i8); 8] = [
        (1, 2),
        (2, 1),
        (2, -1),
        (1, -2),
        (-1, -2),
        (-2, -1),
        (-2, 1),
        (-1, 2),
    ];
    const KING_STEPS: [(i8, i8); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    const BISHOP_DIRS: [(i8, i8); 4] = [(1, 1), (-1, 1), (1, -1), (-1, -1)];
    const ROOK_DIRS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

    pub fn knight(square: Square) -> Bitboard {
        leaper(square, &KNIGHT_STEPS)
    }
    pub fn king(square: Square) -> Bitboard {
        leaper(square, &KING_STEPS)
    }

    /// Squares attacked by a pawn on `square` (does not depend on occupancy).
    pub fn pawn(square: Square, color: Color) -> Bitboard {
        let dr = if color == Color::White { 1 } else { -1 };
        [(-1, dr), (1, dr)]
            .into_iter()
            .filter_map(|(df, rank)| square.offset(df, rank))
            .fold(0, |a, s| a | s.bit())
    }

    pub fn bishop(square: Square, occupied: Bitboard) -> Bitboard {
        sliders(square, occupied, &BISHOP_DIRS)
    }
    pub fn rook(square: Square, occupied: Bitboard) -> Bitboard {
        sliders(square, occupied, &ROOK_DIRS)
    }
    pub fn queen(square: Square, occupied: Bitboard) -> Bitboard {
        bishop(square, occupied) | rook(square, occupied)
    }

    fn leaper(square: Square, steps: &[(i8, i8)]) -> Bitboard {
        steps
            .iter()
            .filter_map(|&(df, dr)| square.offset(df, dr))
            .fold(0, |a, s| a | s.bit())
    }

    /// Portable ray-walk reference implementation; includes the first blocker.
    fn sliders(square: Square, occupied: Bitboard, directions: &[(i8, i8)]) -> Bitboard {
        let mut result = 0;
        for &(df, dr) in directions {
            let mut cursor = square;
            while let Some(next) = cursor.offset(df, dr) {
                result |= next.bit();
                if occupied & next.bit() != 0 {
                    break;
                }
                cursor = next;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{Bitboard, Color, Square, attacks};

    fn sq(s: &str) -> Square {
        Square::from_name(s).unwrap()
    }

    #[test]
    fn square_names_and_indices() {
        assert_eq!(sq("a1").index(), 0);
        assert_eq!(sq("h8").index(), 63);
        assert_eq!(sq("c5").to_string(), "c5");
        assert!(Square::from_name("i1").is_none());
        assert!(Square::from_name("a0").is_none());
    }

    #[test]
    fn start_position_leaper_attacks() {
        assert_eq!(attacks::knight(sq("b1")), mask(&["a3", "c3", "d2"]));
        assert_eq!(
            attacks::king(sq("e1")),
            mask(&["d1", "f1", "d2", "e2", "f2"])
        );
        assert_eq!(attacks::pawn(sq("e2"), Color::White), mask(&["d3", "f3"]));
        assert_eq!(attacks::pawn(sq("e7"), Color::Black), mask(&["d6", "f6"]));
    }

    #[test]
    fn rook_ray_stops_after_and_includes_blocker() {
        let occupied = sq("a4").bit() | sq("d1").bit();
        assert_eq!(
            attacks::rook(sq("a1"), occupied),
            mask(&["a2", "a3", "a4", "b1", "c1", "d1"])
        );
    }

    fn mask(squares: &[&str]) -> Bitboard {
        squares.iter().fold(0, |bb, s| bb | sq(s).bit())
    }
}
