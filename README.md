# Rookery

Rookery is a small, from-scratch Rust chess engine at the v1 release-readiness stage. It uses bitboards and portable ray-walking sliding attacks, with no third-party crate dependencies. It is intended as a correctness-oriented UCI engine rather than a competitive one.

## Build and run

```sh
cargo test --all-targets
cargo run --release
```

The executable supports the UCI handshake, `isready`, `ucinewgame`, `position startpos` and `position fen ...` with move lists, `go depth N`, `go movetime N`, basic `wtime`/`btime` plus increments, `stop`, `quit`, and `go perft N` (root divide output followed by a total). Searches run on a worker thread; `stop` requests cancellation and the command loop emits one final `bestmove` after the worker completes.

## Play in a browser

Start the local board server (the existing `rookery` UCI binary is unchanged):

```sh
cargo run --bin rookery-web --release
```

Then open [http://127.0.0.1:7878](http://127.0.0.1:7878). The board is a self-contained, dependency-free frontend: select a White piece and a highlighted destination, choose promotions in the dialog, flip the orientation as desired, and choose engine search depth 1–6 before starting a new game. The server keeps one in-memory game session, exposes only `/`, `/style.css`, `/app.js`, and its JSON `/api/state`, `/api/new`, and `/api/move` endpoints, and is intended for local use.

## Implemented in v1

- Board primitives, UCI coordinate-move parsing/formatting, and structural FEN parsing and serialization.
- Legal move generation, including castling, en passant, promotions, check, checkmate, and stalemate handling.
- Reversible make/unmake state, including rule counters, castling rights, and en-passant state.
- Perft and root divide coverage for start position, Kiwipete, standard regression fixtures, pinned moves, en-passant discovered checks, and castling transit/rights cases.
- Iterative-deepening negamax with alpha-beta, capture/promotion quiescence, material/positional evaluation, basic capture ordering, and a fixed-size direct-mapped transposition table scoped to one search.
- Search draw handling for threefold repetition from the supplied UCI game history and current search path, the 100-halfmove threshold, and conservative insufficient-material positions.
- Repetition identity that includes an en-passant target only when the side to move has a legal en-passant capture.
- UCI search cancellation through `stop`, including ordered handling of commands queued while a search is being cancelled.

## Known limitations

- Rookery is a simple v1 engine, not a competitive engine. Sliding attacks use ray walking; move ordering is limited; the transposition table is fixed-size/direct-mapped and discarded for each search. There is no opening book, pondering, tablebase support, or advanced search heuristics.
- Time allocation is deliberately basic. The supported `go` options are limited to depth, movetime, and side-to-move clock/increment inputs; unsupported UCI options are ignored.
- FEN parsing validates field structure, piece placement syntax, canonical castling-field spelling, counters, and en-passant rank, but does not validate all chess-position invariants, castling-piece consistency, or reachability.
- Draw adjudication is engine behavior rather than full claimable-draw protocol support. Insufficient-material recognition is intentionally conservative (bare kings, a single minor, or bishops all on one square color); broader dead-position analysis is not implemented.
- The engine tracks repetition history supplied through the current UCI `position` command and the search path. It has no persistent game database or recovery of history omitted by a GUI.

## V2 benchmarking and roadmap

The dependency-free Python UCI gauntlet in [`docs/v2-benchmark.md`](docs/v2-benchmark.md) runs Rookery against Stockfish or another UCI engine from a committed color-paired FEN suite, records each game as JSONL/PGN-like data, and emits a machine-readable W/D/L report. It has no Stockfish requirement for repository tests; Rookery can play itself in the documented smoke mode.

V2 optimization work is deliberately benchmark-led and prioritized as follows:

1. **Movement speed:** profile and improve move generation/make-unmake and sliding attacks while retaining perft correctness.
2. **Search ordering and selectivity:** strengthen ordering, iterative-search information, pruning/reductions, and transposition-table use with tactical regression coverage.
3. **Evaluation:** improve calibrated positional evaluation and testing before increasing complexity.
4. **Later NNUE and parallelism:** consider an NNUE evaluator and parallel search only after the preceding measurements establish a stable baseline and correctness/performance trade-offs.
