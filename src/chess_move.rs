use crate::Square;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Promotion {
    Queen,
    Rook,
    Bishop,
    Knight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub promotion: Option<Promotion>,
}

impl Move {
    pub const fn new(from: Square, to: Square, promotion: Option<Promotion>) -> Self {
        Self {
            from,
            to,
            promotion,
        }
    }

    pub fn from_uci(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 4 && bytes.len() != 5 {
            return None;
        }
        let from = Square::from_name(std::str::from_utf8(&bytes[..2]).ok()?)?;
        let to = Square::from_name(std::str::from_utf8(&bytes[2..4]).ok()?)?;
        let promotion = if bytes.len() == 5 {
            Some(match bytes[4].to_ascii_lowercase() {
                b'q' => Promotion::Queen,
                b'r' => Promotion::Rook,
                b'b' => Promotion::Bishop,
                b'n' => Promotion::Knight,
                _ => return None,
            })
        } else {
            None
        };
        Some(Self {
            from,
            to,
            promotion,
        })
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.from, self.to)?;
        if let Some(p) = self.promotion {
            write!(
                f,
                "{}",
                match p {
                    Promotion::Queen => 'q',
                    Promotion::Rook => 'r',
                    Promotion::Bishop => 'b',
                    Promotion::Knight => 'n',
                }
            )?;
        }
        Ok(())
    }
}
