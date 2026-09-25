//! Rookery: an experimental, from-scratch bitboard chess engine.
//!
//! The crate intentionally starts with explicit board primitives and a slow,
//! simple attack implementation. Optimized sliding attacks can be introduced
//! behind this API and checked against the ray-walking reference.

pub mod bitboard;
pub mod chess_move;
pub mod hash;
pub mod movegen;
pub mod position;
pub mod search;
pub mod web;

pub use bitboard::{Bitboard, Color, Piece, PieceKind, Square, attacks};
pub use position::Position;
