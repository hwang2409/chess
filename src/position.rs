use crate::chess_move::{Move, Promotion};
use crate::{Bitboard, Color, Piece, PieceKind, Square};

pub const WHITE_KINGSIDE: u8 = 1;
pub const WHITE_QUEENSIDE: u8 = 2;
pub const BLACK_KINGSIDE: u8 = 4;
pub const BLACK_QUEENSIDE: u8 = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    squares: [Option<Piece>; 64],
    pieces: [[Bitboard; 6]; 2],
    occupied: [Bitboard; 2],
    side_to_move: Color,
    castling: u8,
    en_passant: Option<Square>,
    halfmove_clock: u16,
    fullmove_number: u16,
}

#[derive(Clone, Debug)]
pub struct Undo {
    captured: Option<(Square, Piece)>,
    moved: Piece,
    castling: u8,
    en_passant: Option<Square>,
    halfmove_clock: u16,
    fullmove_number: u16,
}

impl Position {
    pub fn empty() -> Self {
        Self {
            squares: [None; 64],
            pieces: [[0; 6]; 2],
            occupied: [0; 2],
            side_to_move: Color::White,
            castling: 0,
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 1,
        }
    }
    pub fn startpos() -> Self {
        Self::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap()
    }

    pub fn from_fen(fen: &str) -> Result<Self, String> {
        let fields: Vec<_> = fen.split_whitespace().collect();
        if fields.len() != 6 {
            return Err("FEN must contain six fields".into());
        }
        let mut p = Self::empty();
        let ranks: Vec<_> = fields[0].split('/').collect();
        if ranks.len() != 8 {
            return Err("FEN board must contain eight ranks".into());
        }
        for (fen_rank, row) in ranks.iter().enumerate() {
            let mut file = 0u8;
            for ch in row.chars() {
                if let Some(n) = ch.to_digit(10) {
                    if n == 0 || n > 8 {
                        return Err("invalid empty-square count".into());
                    }
                    file = file.checked_add(n as u8).ok_or("rank overflow")?;
                } else {
                    let color = if ch.is_ascii_uppercase() {
                        Color::White
                    } else {
                        Color::Black
                    };
                    let kind = match ch.to_ascii_lowercase() {
                        'p' => PieceKind::Pawn,
                        'n' => PieceKind::Knight,
                        'b' => PieceKind::Bishop,
                        'r' => PieceKind::Rook,
                        'q' => PieceKind::Queen,
                        'k' => PieceKind::King,
                        _ => return Err("invalid FEN piece".into()),
                    };
                    if file >= 8 {
                        return Err("rank has too many squares".into());
                    }
                    p.set_piece(
                        Square::from_coords(file, 7 - fen_rank as u8).unwrap(),
                        Some(Piece { color, kind }),
                    );
                    file += 1;
                }
            }
            if file != 8 {
                return Err("rank does not contain eight squares".into());
            }
        }
        p.side_to_move = match fields[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _ => return Err("invalid active color".into()),
        };
        if fields[2] != "-" {
            for ch in fields[2].chars() {
                p.castling |= match ch {
                    'K' => WHITE_KINGSIDE,
                    'Q' => WHITE_QUEENSIDE,
                    'k' => BLACK_KINGSIDE,
                    'q' => BLACK_QUEENSIDE,
                    _ => return Err("invalid castling rights".into()),
                };
            }
        }
        p.en_passant = if fields[3] == "-" {
            None
        } else {
            let sq = Square::from_name(fields[3]).ok_or("invalid en-passant square")?;
            let rank = if p.side_to_move == Color::White { 5 } else { 2 };
            if sq.rank() != rank {
                return Err("en-passant square is on the wrong rank for the active color".into());
            }
            Some(sq)
        };
        p.halfmove_clock = fields[4].parse().map_err(|_| "invalid halfmove clock")?;
        p.fullmove_number = fields[5].parse().map_err(|_| "invalid fullmove number")?;
        if p.fullmove_number == 0 {
            return Err("fullmove number must be positive".into());
        }
        Ok(p)
    }

    pub fn to_fen(&self) -> String {
        let mut board = String::new();
        for rank in (0..8).rev() {
            if rank != 7 {
                board.push('/');
            }
            let mut empty = 0u8;
            for file in 0..8 {
                let sq = Square::from_coords(file, rank).unwrap();
                if let Some(piece) = self.piece_at(sq) {
                    if empty > 0 {
                        board.push(char::from(b'0' + empty));
                        empty = 0;
                    }
                    let c = match piece.kind {
                        PieceKind::Pawn => 'p',
                        PieceKind::Knight => 'n',
                        PieceKind::Bishop => 'b',
                        PieceKind::Rook => 'r',
                        PieceKind::Queen => 'q',
                        PieceKind::King => 'k',
                    };
                    board.push(if piece.color == Color::White {
                        c.to_ascii_uppercase()
                    } else {
                        c
                    });
                } else {
                    empty += 1;
                }
            }
            if empty > 0 {
                board.push(char::from(b'0' + empty));
            }
        }
        let mut rights = String::new();
        for (mask, c) in [
            (WHITE_KINGSIDE, 'K'),
            (WHITE_QUEENSIDE, 'Q'),
            (BLACK_KINGSIDE, 'k'),
            (BLACK_QUEENSIDE, 'q'),
        ] {
            if self.castling & mask != 0 {
                rights.push(c);
            }
        }
        if rights.is_empty() {
            rights.push('-');
        }
        format!(
            "{} {} {} {} {} {}",
            board,
            if self.side_to_move == Color::White {
                "w"
            } else {
                "b"
            },
            rights,
            self.en_passant.map_or("-".into(), |s| s.to_string()),
            self.halfmove_clock,
            self.fullmove_number
        )
    }

    pub fn piece_at(&self, s: Square) -> Option<Piece> {
        self.squares[s.index() as usize]
    }
    pub fn pieces(&self, c: Color, k: PieceKind) -> Bitboard {
        self.pieces[c as usize][k as usize]
    }
    pub fn occupied_by(&self, c: Color) -> Bitboard {
        self.occupied[c as usize]
    }
    pub fn occupied(&self) -> Bitboard {
        self.occupied[0] | self.occupied[1]
    }
    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }
    pub fn castling_rights(&self) -> u8 {
        self.castling
    }
    pub fn en_passant_square(&self) -> Option<Square> {
        self.en_passant
    }
    pub fn halfmove_clock(&self) -> u16 {
        self.halfmove_clock
    }
    pub fn fullmove_number(&self) -> u16 {
        self.fullmove_number
    }
    pub fn zobrist_hash(&self) -> u64 {
        crate::hash::hash_position(self)
    }
    /// Whether no legal continuation can produce checkmate due to the remaining material.
    /// This intentionally recognizes only conservative, standard dead-position cases.
    pub fn is_insufficient_material(&self) -> bool {
        let pawns =
            self.pieces(Color::White, PieceKind::Pawn) | self.pieces(Color::Black, PieceKind::Pawn);
        let rooks =
            self.pieces(Color::White, PieceKind::Rook) | self.pieces(Color::Black, PieceKind::Rook);
        let queens = self.pieces(Color::White, PieceKind::Queen)
            | self.pieces(Color::Black, PieceKind::Queen);
        if pawns | rooks | queens != 0 {
            return false;
        }
        let knights = self.pieces(Color::White, PieceKind::Knight)
            | self.pieces(Color::Black, PieceKind::Knight);
        let bishops = self.pieces(Color::White, PieceKind::Bishop)
            | self.pieces(Color::Black, PieceKind::Bishop);
        if bishops == 0 {
            return knights.count_ones() <= 1;
        }
        if knights != 0 {
            return false;
        }
        let light_squares = 0x55aa_55aa_55aa_55aau64;
        bishops & light_squares == 0 || bishops & !light_squares == 0
    }

    pub fn king_square(&self, color: Color) -> Option<Square> {
        let bb = self.pieces(color, PieceKind::King);
        if bb.count_ones() != 1 {
            return None;
        }
        Square::new(bb.trailing_zeros() as u8)
    }

    pub fn make_move(&mut self, mv: Move) -> Undo {
        let moved = self
            .piece_at(mv.from)
            .expect("move source must contain a piece");
        let mut undo = Undo {
            captured: None,
            moved,
            castling: self.castling,
            en_passant: self.en_passant,
            halfmove_clock: self.halfmove_clock,
            fullmove_number: self.fullmove_number,
        };
        let capture_sq = if moved.kind == PieceKind::Pawn
            && Some(mv.to) == self.en_passant
            && self.piece_at(mv.to).is_none()
        {
            mv.to
                .offset(0, if moved.color == Color::White { -1 } else { 1 })
                .unwrap()
        } else {
            mv.to
        };
        undo.captured = self.piece_at(capture_sq).map(|piece| (capture_sq, piece));
        self.set_piece(mv.from, None);
        self.set_piece(capture_sq, None);
        let placed = if let Some(promo) = mv.promotion {
            Piece {
                color: moved.color,
                kind: match promo {
                    Promotion::Queen => PieceKind::Queen,
                    Promotion::Rook => PieceKind::Rook,
                    Promotion::Bishop => PieceKind::Bishop,
                    Promotion::Knight => PieceKind::Knight,
                },
            }
        } else {
            moved
        };
        self.set_piece(mv.to, Some(placed));
        if moved.kind == PieceKind::King && mv.from.file().abs_diff(mv.to.file()) == 2 {
            let (rf, rt) = if mv.to.file() == 6 { (7, 5) } else { (0, 3) };
            let a = Square::from_coords(rf, mv.from.rank()).unwrap();
            let b = Square::from_coords(rt, mv.from.rank()).unwrap();
            let rook = self.piece_at(a);
            self.set_piece(a, None);
            self.set_piece(b, rook);
        }
        self.en_passant =
            if moved.kind == PieceKind::Pawn && mv.from.rank().abs_diff(mv.to.rank()) == 2 {
                mv.from
                    .offset(0, if moved.color == Color::White { 1 } else { -1 })
            } else {
                None
            };
        if moved.kind == PieceKind::King {
            self.castling &= if moved.color == Color::White {
                !(WHITE_KINGSIDE | WHITE_QUEENSIDE)
            } else {
                !(BLACK_KINGSIDE | BLACK_QUEENSIDE)
            };
        }
        for (sq, right) in [
            (Square::from_name("a1").unwrap(), WHITE_QUEENSIDE),
            (Square::from_name("h1").unwrap(), WHITE_KINGSIDE),
            (Square::from_name("a8").unwrap(), BLACK_QUEENSIDE),
            (Square::from_name("h8").unwrap(), BLACK_KINGSIDE),
        ] {
            if mv.from == sq || mv.to == sq {
                self.castling &= !right;
            }
        }
        self.halfmove_clock = if moved.kind == PieceKind::Pawn || undo.captured.is_some() {
            0
        } else {
            self.halfmove_clock.saturating_add(1)
        };
        if moved.color == Color::Black {
            self.fullmove_number = self.fullmove_number.saturating_add(1);
        }
        self.side_to_move = self.side_to_move.opposite();
        undo
    }

    pub fn unmake_move(&mut self, mv: Move, undo: Undo) {
        self.side_to_move = self.side_to_move.opposite();
        self.set_piece(mv.to, None);
        self.set_piece(mv.from, Some(undo.moved));
        if let Some((sq, piece)) = undo.captured {
            self.set_piece(sq, Some(piece));
        }
        if undo.moved.kind == PieceKind::King && mv.from.file().abs_diff(mv.to.file()) == 2 {
            let (rf, rt) = if mv.to.file() == 6 { (7, 5) } else { (0, 3) };
            let a = Square::from_coords(rf, mv.from.rank()).unwrap();
            let b = Square::from_coords(rt, mv.from.rank()).unwrap();
            let rook = self.piece_at(b);
            self.set_piece(b, None);
            self.set_piece(a, rook);
        }
        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.fullmove_number = undo.fullmove_number;
    }

    fn set_piece(&mut self, square: Square, piece: Option<Piece>) {
        let i = square.index() as usize;
        if let Some(old) = self.squares[i] {
            self.pieces[old.color as usize][old.kind as usize] &= !square.bit();
            self.occupied[old.color as usize] &= !square.bit();
        }
        self.squares[i] = piece;
        if let Some(new) = piece {
            self.pieces[new.color as usize][new.kind as usize] |= square.bit();
            self.occupied[new.color as usize] |= square.bit();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Position;
    use crate::movegen::legal_moves;
    #[test]
    fn fen_round_trip() {
        let fen = "r3k2r/8/8/3pP3/8/8/8/R3K2R w KQkq d6 7 12";
        assert_eq!(Position::from_fen(fen).unwrap().to_fen(), fen);
        assert_eq!(
            Position::startpos().to_fen(),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
        );
    }
    #[test]
    fn rejects_invalid_en_passant_ranks() {
        assert!(Position::from_fen("4k3/8/8/8/4P3/8/8/4K3 w - e4 0 1").is_err());
        assert!(Position::from_fen("4k3/8/8/8/8/8/4p3/4K3 b - e5 0 1").is_err());
    }
    #[test]
    fn insufficient_material_recognizes_only_supported_dead_positions() {
        for fen in [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/3NK3 w - - 0 1",
            "4k3/8/8/8/8/8/8/2B1K1B1 w - - 0 1",
            "4k3/8/8/8/8/8/8/1b2KB2 w - - 0 1",
        ] {
            assert!(
                Position::from_fen(fen).unwrap().is_insufficient_material(),
                "{fen}"
            );
        }
        for fen in [
            "4k3/8/8/8/8/8/8/2NNK3 w - - 0 1",
            "4k3/8/8/8/8/8/8/2B1KBb1 w - - 0 1",
            "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/R3K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/Q3K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/2B1KN2 w - - 0 1",
        ] {
            assert!(
                !Position::from_fen(fen).unwrap().is_insufficient_material(),
                "{fen}"
            );
        }
    }

    #[test]
    fn make_unmake_restores_special_moves() {
        let mut p = Position::from_fen("r3k2r/P2p4/8/3pP3/8/8/8/R3K2R w KQkq d6 0 1").unwrap();
        let original = p.clone();
        let moves = legal_moves(&mut p);
        for text in ["e5d6", "a7a8q", "e1g1", "e1c1"] {
            if let Some(mv) = moves.iter().find(|m| m.to_string() == text).copied() {
                let undo = p.make_move(mv);
                p.unmake_move(mv, undo);
                assert_eq!(p, original, "{text}");
            }
        }
    }
}
