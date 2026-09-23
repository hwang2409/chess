use crate::chess_move::Move;
use crate::hash::repetition_key;
use crate::movegen::{in_check, legal_moves};
use crate::{Color, PieceKind, Position};
use std::time::{Duration, Instant};

const INF: i32 = 32_000;
const MATE: i32 = 30_000;
const MAX_PLY: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub depth: u8,
    pub nodes: u64,
}

pub struct Searcher {
    nodes: u64,
    deadline: Option<Instant>,
    aborted: bool,
    path: Vec<u64>,
}

impl Searcher {
    pub fn new() -> Self {
        Self {
            nodes: 0,
            deadline: None,
            aborted: false,
            path: Vec::new(),
        }
    }
    pub fn clear(&mut self) {}

    pub fn search(
        &mut self,
        position: &mut Position,
        max_depth: u8,
        limit: Option<Duration>,
    ) -> SearchResult {
        self.search_with_history(position, max_depth, limit, &[])
    }

    /// Search using repetition keys for game positions preceding `position`.
    /// The current position is added internally, so history must exclude it.
    pub fn search_with_history(
        &mut self,
        position: &mut Position,
        max_depth: u8,
        limit: Option<Duration>,
        history: &[u64],
    ) -> SearchResult {
        self.nodes = 0;
        self.aborted = false;
        self.deadline = limit.and_then(|d| Instant::now().checked_add(d));
        self.path.clear();
        self.path.extend_from_slice(history);
        self.path.push(repetition_key(position));
        if self.is_repetition() {
            return SearchResult {
                best_move: legal_moves(position).first().copied(),
                score: 0,
                depth: 0,
                nodes: 0,
            };
        }
        let mut moves = legal_moves(position);
        moves.sort_by_key(|m| !self.is_capture(position, *m));
        let mut result = SearchResult {
            best_move: moves.first().copied(),
            score: 0,
            depth: 0,
            nodes: 0,
        };
        if moves.is_empty() {
            result.score = if in_check(position, position.side_to_move()) {
                -MATE
            } else {
                0
            };
            return result;
        }
        for depth in 1..=max_depth {
            let mut best = None;
            let mut alpha = -INF;
            for mv in legal_moves(position) {
                if self.expired() {
                    break;
                }
                let undo = position.make_move(mv);
                self.path.push(repetition_key(position));
                let score = -self.negamax(position, depth.saturating_sub(1), -INF, -alpha, 1);
                self.path.pop();
                position.unmake_move(mv, undo);
                if !self.aborted && score > alpha {
                    alpha = score;
                    best = Some(mv);
                }
            }
            if self.aborted {
                break;
            }
            if let Some(mv) = best {
                result.best_move = Some(mv);
                result.score = alpha;
                result.depth = depth;
            }
        }
        result.nodes = self.nodes;
        result
    }

    fn negamax(
        &mut self,
        p: &mut Position,
        depth: u8,
        mut alpha: i32,
        beta: i32,
        ply: usize,
    ) -> i32 {
        self.nodes += 1;
        if self.expired() {
            return 0;
        }
        if self.is_repetition() {
            return 0;
        }
        if p.halfmove_clock() >= 100 {
            return 0;
        }
        if depth == 0 || ply >= MAX_PLY {
            return self.quiescence(p, alpha, beta, ply);
        }
        let check = in_check(p, p.side_to_move());
        let moves = legal_moves(p);
        if moves.is_empty() {
            return if check { -MATE + ply as i32 } else { 0 };
        }
        let mut best = -INF;
        for mv in moves {
            let undo = p.make_move(mv);
            self.path.push(repetition_key(p));
            let score = -self.negamax(p, depth - 1, -beta, -alpha, ply + 1);
            self.path.pop();
            p.unmake_move(mv, undo);
            if self.aborted {
                return 0;
            }
            best = best.max(score);
            alpha = alpha.max(score);
            if alpha >= beta {
                break;
            }
        }
        best
    }

    fn quiescence(&mut self, p: &mut Position, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        self.nodes += 1;
        if self.expired() || self.is_repetition() || p.halfmove_clock() >= 100 {
            return 0;
        }
        let check = in_check(p, p.side_to_move());
        let stand = evaluate(p);
        if !check {
            if stand >= beta {
                return beta;
            }
            alpha = alpha.max(stand);
        }
        if ply >= MAX_PLY {
            return if check { stand } else { alpha };
        }
        let moves = legal_moves(p);
        if moves.is_empty() {
            return if check { -MATE + ply as i32 } else { alpha };
        }
        for mv in moves {
            if !check && !self.is_capture(p, mv) && mv.promotion.is_none() {
                continue;
            }
            let undo = p.make_move(mv);
            self.path.push(repetition_key(p));
            let score = -self.quiescence(p, -beta, -alpha, ply + 1);
            self.path.pop();
            p.unmake_move(mv, undo);
            if self.aborted {
                return 0;
            }
            if score >= beta {
                return beta;
            }
            alpha = alpha.max(score);
        }
        alpha
    }

    fn is_repetition(&self) -> bool {
        self.path.last().is_some_and(|key| {
            self.path
                .iter()
                .filter(|candidate| *candidate == key)
                .count()
                >= 3
        })
    }
    fn is_capture(&self, p: &Position, mv: Move) -> bool {
        p.piece_at(mv.to).is_some()
            || (p
                .piece_at(mv.from)
                .is_some_and(|x| x.kind == PieceKind::Pawn)
                && Some(mv.to) == p.en_passant_square())
    }
    fn expired(&mut self) -> bool {
        if self.deadline.is_some_and(|d| Instant::now() >= d) {
            self.aborted = true;
        }
        self.aborted
    }
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}

fn evaluate(p: &Position) -> i32 {
    let values = [100, 320, 330, 500, 900, 0];
    let mut score = 0;
    for color in [Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        for kind in [
            PieceKind::Pawn,
            PieceKind::Knight,
            PieceKind::Bishop,
            PieceKind::Rook,
            PieceKind::Queen,
            PieceKind::King,
        ] {
            let mut bb = p.pieces(color, kind);
            score += sign * values[kind as usize] * bb.count_ones() as i32;
            while bb != 0 {
                let sq = bb.trailing_zeros() as u8;
                bb &= bb - 1;
                let file = (sq % 8) as i32;
                let rank = (sq / 8) as i32;
                let advancement = if color == Color::White {
                    rank
                } else {
                    7 - rank
                };
                if kind == PieceKind::Pawn {
                    score += sign * advancement * 4;
                }
                if matches!(kind, PieceKind::Knight | PieceKind::Bishop) {
                    let center = 7 - ((file * 2 - 7).abs() + (rank * 2 - 7).abs()) / 2;
                    score += sign * center * 3;
                }
            }
        }
    }
    if p.side_to_move() == Color::White {
        score
    } else {
        -score
    }
}

#[cfg(test)]
mod tests {
    use super::Searcher;
    use crate::movegen::legal_moves;
    use crate::{Position, hash};
    use std::time::Duration;

    #[test]
    fn start_position_search_returns_legal_move() {
        let mut p = Position::startpos();
        let result = Searcher::new().search(&mut p, 3, None);
        assert_eq!(result.depth, 3);
        assert!(legal_moves(&mut p).contains(&result.best_move.unwrap()));
        assert!(result.nodes > 0);
    }
    #[test]
    fn repetition_identity_excludes_halfmove_clock_but_tt_key_does_not() {
        let mut p = Position::startpos();
        let start_rep = hash::repetition_key(&p);
        let start_tt = p.zobrist_hash();
        for text in ["g1f3", "g8f6", "f3g1", "f6g8"] {
            let mv = legal_moves(&mut p)
                .into_iter()
                .find(|m| m.to_string() == text)
                .unwrap();
            p.make_move(mv);
        }
        assert_eq!(hash::repetition_key(&p), start_rep);
        assert_ne!(p.zobrist_hash(), start_tt);
    }
    #[test]
    fn repetition_requires_three_occurrences_including_current_position() {
        let mut p = Position::startpos();
        let key = hash::repetition_key(&p);
        let once = Searcher::new().search_with_history(&mut p, 1, None, &[key]);
        assert_eq!(once.depth, 1);

        let twice = Searcher::new().search_with_history(&mut p, 1, None, &[key, key]);
        assert_eq!(twice.depth, 0);
        assert_eq!(twice.score, 0);
    }

    #[test]
    fn tiny_time_limit_still_returns_a_legal_fallback() {
        let mut p = Position::startpos();
        let result = Searcher::new().search(&mut p, 20, Some(Duration::from_millis(1)));
        assert!(legal_moves(&mut p).contains(&result.best_move.unwrap()));
    }
}
