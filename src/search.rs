use crate::chess_move::Move;
use crate::hash::repetition_key;
use crate::movegen::{has_legal_move, in_check, legal_moves};
use crate::{Color, PieceKind, Position};
use std::time::{Duration, Instant};

const INF: i32 = 32_000;
const MATE: i32 = 30_000;
const MAX_PLY: usize = 64;
const TT_SIZE: usize = 1 << 16;

#[derive(Clone, Copy, Debug)]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub depth: u8,
    pub nodes: u64,
}

#[derive(Clone, Copy)]
enum Bound {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy)]
struct TranspositionEntry {
    key: u64,
    history_key: u64,
    depth: u8,
    score: i32,
    bound: Bound,
    best_move: Option<Move>,
}

/// A fixed-size, direct-mapped table retained only for one iterative search.
///
/// Entries also carry a compact key for the current repetition path, so a
/// bound is not reused with different draw history. Keeping the table scoped
/// this way avoids carrying bounds across unrelated game histories while still
/// allowing each deeper iteration to reuse completed work.
struct TranspositionTable {
    entries: Vec<Option<TranspositionEntry>>,
}

impl TranspositionTable {
    fn new() -> Self {
        Self {
            entries: vec![None; TT_SIZE],
        }
    }

    fn clear(&mut self) {
        self.entries.fill(None);
    }

    fn get(&self, key: u64, history_key: u64) -> Option<TranspositionEntry> {
        self.entries[key as usize & (TT_SIZE - 1)]
            .filter(|entry| entry.key == key && entry.history_key == history_key)
    }

    fn store(&mut self, entry: TranspositionEntry) {
        let slot = &mut self.entries[entry.key as usize & (TT_SIZE - 1)];
        if slot.is_none_or(|old| entry.depth >= old.depth) {
            *slot = Some(entry);
        }
    }
}

pub struct Searcher {
    nodes: u64,
    deadline: Option<Instant>,
    aborted: bool,
    path: Vec<u64>,
    tt: TranspositionTable,
    tt_enabled: bool,
}

impl Searcher {
    pub fn new() -> Self {
        Self {
            nodes: 0,
            deadline: None,
            aborted: false,
            path: Vec::new(),
            tt: TranspositionTable::new(),
            tt_enabled: true,
        }
    }

    #[cfg(test)]
    fn without_transposition_table() -> Self {
        Self {
            tt_enabled: false,
            ..Self::new()
        }
    }

    pub fn clear(&mut self) {
        self.tt.clear();
    }

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
        self.tt.clear();
        self.path.clear();
        let reversible_history_len = usize::from(position.halfmove_clock());
        self.path
            .extend_from_slice(&history[history.len().saturating_sub(reversible_history_len)..]);
        self.path.push(repetition_key(position));
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
        if position.is_insufficient_material()
            || self.is_repetition()
            || position.halfmove_clock() >= 100
        {
            result.best_move = None;
            result.score = 0;
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
        mut beta: i32,
        ply: usize,
    ) -> i32 {
        self.nodes += 1;
        if self.expired() {
            return 0;
        }
        if depth == 0 || ply >= MAX_PLY {
            return self.quiescence(p, alpha, beta, ply);
        }
        // Checkmate and stalemate take precedence over draw claims at this node.
        let check = in_check(p, p.side_to_move());
        let mut moves = legal_moves(p);
        if moves.is_empty() {
            return if check { -MATE + ply as i32 } else { 0 };
        }
        if p.is_insufficient_material() || self.is_repetition() || p.halfmove_clock() >= 100 {
            return 0;
        }

        let key = p.zobrist_hash();
        let history_key = self.repetition_path_key();
        let tt_entry = self
            .tt_enabled
            .then(|| self.tt.get(key, history_key))
            .flatten();
        if let Some(entry) = tt_entry {
            if entry.depth >= depth {
                let score = score_from_tt(entry.score, ply);
                match entry.bound {
                    Bound::Exact => return score,
                    Bound::Lower => alpha = alpha.max(score),
                    Bound::Upper => beta = beta.min(score),
                }
                if alpha >= beta {
                    return score;
                }
            }
            if let Some(best_move) = entry.best_move
                && let Some(index) = moves.iter().position(|mv| *mv == best_move)
            {
                moves.swap(0, index);
            }
        }

        let alpha_bound = alpha;
        let beta_bound = beta;
        let mut best = -INF;
        let mut best_move = None;
        for mv in moves {
            let undo = p.make_move(mv);
            self.path.push(repetition_key(p));
            let score = -self.negamax(p, depth - 1, -beta, -alpha, ply + 1);
            self.path.pop();
            p.unmake_move(mv, undo);
            if self.aborted {
                return 0;
            }
            if score > best {
                best = score;
                best_move = Some(mv);
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                break;
            }
        }

        if self.tt_enabled {
            let bound = if best <= alpha_bound {
                Bound::Upper
            } else if best >= beta_bound {
                Bound::Lower
            } else {
                Bound::Exact
            };
            self.tt.store(TranspositionEntry {
                key,
                history_key,
                depth,
                score: score_to_tt(best, ply),
                bound,
                best_move,
            });
        }
        best
    }

    fn quiescence(&mut self, p: &mut Position, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        self.nodes += 1;
        if self.expired() {
            return 0;
        }
        let check = in_check(p, p.side_to_move());
        let moves = if check {
            let moves = legal_moves(p);
            if moves.is_empty() {
                return -MATE + ply as i32;
            }
            Some(moves)
        } else {
            if !has_legal_move(p) {
                return 0;
            }
            None
        };
        if p.is_insufficient_material() || self.is_repetition() || p.halfmove_clock() >= 100 {
            return 0;
        }
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
        let moves = moves.unwrap_or_else(|| legal_moves(p));
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

    fn repetition_path_key(&self) -> u64 {
        self.path.iter().fold(0, |key, position_key| {
            key.wrapping_add(mix_history_key(*position_key))
        })
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

fn mix_history_key(mut key: u64) -> u64 {
    key ^= key >> 30;
    key = key.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    key ^= key >> 27;
    key = key.wrapping_mul(0x94d0_49bb_1331_11eb);
    key ^ (key >> 31)
}

fn score_to_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE - MAX_PLY as i32 {
        score + ply as i32
    } else if score <= -MATE + MAX_PLY as i32 {
        score - ply as i32
    } else {
        score
    }
}

fn score_from_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE - MAX_PLY as i32 {
        score - ply as i32
    } else if score <= -MATE + MAX_PLY as i32 {
        score + ply as i32
    } else {
        score
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
    use super::{MATE, Searcher, score_from_tt, score_to_tt};
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
    fn repetition_requires_three_occurrences_in_reversible_suffix() {
        let mut p = Position::startpos();
        let key = hash::repetition_key(&p);
        let mut history = vec![key];
        for text in [
            "g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8",
        ] {
            let mv = legal_moves(&mut p)
                .into_iter()
                .find(|m| m.to_string() == text)
                .unwrap();
            p.make_move(mv);
            history.push(hash::repetition_key(&p));
        }
        history.pop(); // search adds the current position itself
        let repeated = Searcher::new().search_with_history(&mut p, 1, None, &history);
        assert_eq!(repeated.depth, 0);
        assert_eq!(repeated.score, 0);
        assert_eq!(repeated.best_move, None);

        let mut p =
            Position::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1")
                .unwrap();
        let key = hash::repetition_key(&p);
        // These matching positions are in history before the pawn move; clock 0
        // means none of that prefix can contribute to repetition.
        let beyond_irreversible_prefix =
            Searcher::new().search_with_history(&mut p, 1, None, &[key, key]);
        assert_eq!(beyond_irreversible_prefix.depth, 1);
        assert!(beyond_irreversible_prefix.best_move.is_some());
    }

    #[test]
    fn insufficient_material_is_a_root_and_interior_search_draw() {
        let mut p = Position::from_fen("4k3/8/8/8/8/8/8/3NK3 w - - 0 1").unwrap();
        let result = Searcher::new().search(&mut p, 3, None);
        assert_eq!(result.depth, 0);
        assert_eq!(result.score, 0);
        assert_eq!(result.best_move, None);

        let mut p = Position::from_fen("4k3/8/8/8/8/8/8/3NK3 w - - 0 1").unwrap();
        let mut searcher = Searcher::new();
        assert_eq!(searcher.negamax(&mut p, 2, -32_000, 32_000, 1), 0);
        assert_eq!(searcher.quiescence(&mut p, -32_000, 32_000, 1), 0);
    }

    #[test]
    fn root_halfmove_clock_draw_returns_no_move() {
        let mut p =
            Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 100 51")
                .unwrap();
        let result = Searcher::new().search_with_history(&mut p, 4, None, &[]);
        assert_eq!(result.depth, 0);
        assert_eq!(result.score, 0);
        assert_eq!(result.best_move, None);
    }

    #[test]
    fn terminal_root_takes_precedence_over_repetition_and_halfmove_draw() {
        for (fen, expected_score) in [
            ("7k/6Q1/6K1/8/8/8/8/8 b - - 100 1", -30_000),
            ("7k/5Q2/6K1/8/8/8/8/8 b - - 100 1", 0),
        ] {
            let mut p = Position::from_fen(fen).unwrap();
            let key = hash::repetition_key(&p);
            let result = Searcher::new().search_with_history(&mut p, 4, None, &[key, key]);
            assert_eq!(result.depth, 0);
            assert_eq!(result.score, expected_score);
            assert_eq!(result.best_move, None);
        }
    }

    #[test]
    // Exercises both negamax and qsearch terminal-first behavior: mate remains a mate,
    // and stalemate remains a draw, even when repetition and the 100-halfmove rule apply.
    fn interior_terminal_positions_precede_repetition_and_halfmove_draws() {
        for (fen, expected) in [
            ("7k/6Q1/6K1/8/8/8/8/8 b - - 100 1", -29_996),
            ("7k/5Q2/6K1/8/8/8/8/8 b - - 100 1", 0),
        ] {
            let mut p = Position::from_fen(fen).unwrap();
            let key = hash::repetition_key(&p);
            let mut searcher = Searcher::new();
            searcher.path = vec![key, key];
            assert_eq!(searcher.negamax(&mut p, 2, -32_000, 32_000, 4), expected);
            assert_eq!(searcher.quiescence(&mut p, -32_000, 32_000, 4), expected);
        }
    }

    #[test]
    fn quiescence_stalemate_precedes_draw_and_stand_pat_cutoff() {
        let mut p = Position::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 100 1").unwrap();
        let original = p.clone();
        let mut searcher = Searcher::new();
        searcher.path = vec![hash::repetition_key(&p); 2];
        assert_eq!(searcher.quiescence(&mut p, -32_000, -31_999, 1), 0);
        assert_eq!(p, original);

        let mut p = Position::startpos();
        let mut searcher = Searcher::new();
        let beta = 0;
        assert_eq!(searcher.quiescence(&mut p, -32_000, beta, 1), beta);
        assert_eq!(searcher.nodes, 1);
    }

    #[test]
    fn insufficient_material_stalemate_is_recognized_as_terminal_at_root_and_qsearch() {
        // Black is stalemated; the lone bishop also qualifies as insufficient material.
        let mut p = Position::from_fen("7k/5B2/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        assert!(p.is_insufficient_material());
        assert!(legal_moves(&mut p).is_empty());
        let root = Searcher::new().search(&mut p, 2, None);
        assert_eq!(root.score, 0);
        assert_eq!(root.best_move, None);

        let mut searcher = Searcher::new();
        assert_eq!(searcher.negamax(&mut p, 2, -32_000, 32_000, 1), 0);
        assert_eq!(searcher.quiescence(&mut p, -32_000, 32_000, 1), 0);
    }

    #[test]
    fn quiescence_returns_draw_for_noncheck_stalemate() {
        // Black to move is stalemated in this position.
        let mut p = Position::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        assert_eq!(Searcher::new().quiescence(&mut p, -32_000, 32_000, 1), 0);
    }

    #[test]
    fn transposition_table_mate_scores_are_ply_normalized() {
        assert_eq!(score_from_tt(score_to_tt(MATE - 9, 7), 7), MATE - 9);
        assert_eq!(score_from_tt(score_to_tt(-MATE + 9, 7), 7), -MATE + 9);
        assert_eq!(score_from_tt(score_to_tt(MATE - 9, 7), 3), MATE - 5);
        assert_eq!(score_from_tt(score_to_tt(-MATE + 9, 7), 3), -MATE + 5);
    }

    #[test]
    fn transposition_table_normalizes_mate_window_boundaries() {
        let positive_boundary = MATE - super::MAX_PLY as i32;
        let negative_boundary = -MATE + super::MAX_PLY as i32;

        assert_eq!(
            score_from_tt(score_to_tt(positive_boundary, 7), 3),
            positive_boundary + 4
        );
        assert_eq!(
            score_from_tt(score_to_tt(negative_boundary, 7), 3),
            negative_boundary - 4
        );
    }

    #[test]
    fn transposition_table_preserves_result_and_reduces_nodes() {
        let mut with_tt_position = Position::startpos();
        let with_tt = Searcher::new().search(&mut with_tt_position, 4, None);
        let mut without_tt_position = Position::startpos();
        let without_tt =
            Searcher::without_transposition_table().search(&mut without_tt_position, 4, None);

        assert_eq!(with_tt.depth, without_tt.depth);
        assert_eq!(with_tt.score, without_tt.score);
        assert_eq!(with_tt.best_move, without_tt.best_move);
        assert_eq!(with_tt_position, without_tt_position);
        assert!(with_tt.nodes < without_tt.nodes);
    }

    #[test]
    fn tiny_time_limit_still_returns_a_legal_fallback() {
        let mut p = Position::startpos();
        let result = Searcher::new().search(&mut p, 20, Some(Duration::from_millis(1)));
        assert!(legal_moves(&mut p).contains(&result.best_move.unwrap()));
    }
}
