# Rookery

Rookery is a from-scratch Rust chess engine in an early v0 state. The core is dependency-light and uses bitboards with portable ray-walking sliding attacks.

## Build and run

```sh
cargo test
cargo run --release
```

The executable supports the basic UCI handshake, `position startpos` / `position fen ...` with move lists, `go depth N`, basic time controls (`movetime`, `wtime`/`btime` and increments), `stop`, and `go perft N` (with root divide output).

## Implemented

- Board primitives, attack generation, FEN parse/serialize, and UCI move parsing.
- Pseudo-legal and legal move generation, including castling, en passant, and promotions.
- Reversible make/unmake state for board and rule counters.
- Perft and divide, checked against start-position depths 1–4 and Kiwipete depths 1–3.
- Iterative-deepening negamax with alpha-beta, a scoped transposition table, capture/promotion quiescence, basic material/positional evaluation, repetition/50-move draw checks, and time limits.

## Known limitations

- This is a correctness-oriented v0, not a competitive engine. Sliding attacks use ray walking; the transposition table is fixed-size and scoped to each search; there are no magic bitboards, move-ordering heuristics beyond captures, or opening book.
- UCI search is synchronous; `stop` cannot interrupt a search while `go` is running. Time checks occur throughout recursive search, but time allocation is intentionally rudimentary.
- Search repetition tracking covers the current search path, not the complete game history supplied by the GUI. The UCI `position` command does not retain earlier game-position hashes for threefold claims.
- Repetition identity currently includes the FEN en-passant file whenever present, even where no legal en-passant capture exists; strict FIDE repetition equivalence can therefore differ in edge cases.
- FEN parsing checks field shape and en-passant rank but does not validate every chess-position invariant (for example, king counts, castling-right consistency, or reachability).
- Draw adjudication is limited to repetition-path and 100-halfmove checks. Insufficient material, claimable-draw protocol behavior, and full game adjudication are not implemented.
