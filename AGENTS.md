# Rookery contributor notes

## Commands

- `cargo fmt --check` — verify Rust formatting.
- `cargo test` — run unit tests plus integration tests (including spawning the UCI binary).
- `cargo clippy -- -D warnings` — lint with warnings treated as errors.
- `cargo run --release` — start the synchronous stdin/stdout UCI engine; e.g. send `uci`, `position startpos`, `go depth 3`, `quit`.
- Target a regression while iterating: `cargo test --test perft_regression`, `cargo test --test make_unmake`, or `cargo test --test uci`.

The crate has no dependencies, uses Rust edition 2024, and has no pinned toolchain or custom Cargo/rustfmt configuration.

## Architecture

- `src/bitboard.rs`: board primitives (`Square` is `a1 = 0` through `h8 = 63`), pieces/colors, and portable ray-walk attack masks.
- `src/chess_move.rs`: UCI move parsing/formatting and promotion representation.
- `src/position.rs`: FEN parsing/serialization and canonical mutable board state. `make_move`/`unmake_move` maintain square, piece, occupancy, rule-counter, castling, and en-passant state.
- `src/movegen.rs`: pseudo-legal generation, legal filtering, attack/check queries, and perft/divide.
- `src/hash.rs`: deterministic Zobrist-style position and repetition keys.
- `src/search.rs`: iterative-deepening negamax with alpha-beta, quiescence, material/positional evaluation, time limits, and repetition-path handling.
- `src/main.rs`: minimal UCI command loop; parses `position`, handles `go depth`/time controls and `go perft`.
- `tests/`: integration coverage for perft fixtures, deterministic make/unmake round trips, and subprocess UCI behavior. `docs/iteration-plan.md` records the v0 correctness-first scope.

## Project conventions

- Preserve the explicit, dependency-free bitboard/reference-attack design unless an optimization is deliberately introduced behind the existing API.
- Treat position mutation as an invariant boundary: every move-generation/search/perft path that calls `make_move` must retain its `Undo`, call `unmake_move`, and leave the input `Position` unchanged.
- Validate chess-rule changes with perft and special-move coverage (castling, en passant, promotion), not only search results. Add integration regressions in `tests/` for externally visible UCI behavior or multi-module invariants.
- Moves are compared as `Move` values and displayed/parsed in lowercase UCI coordinate notation. Keep FEN and UCI parsing failures descriptive `Result<String>` errors.
- Search scores are from the side-to-move perspective; repetition history excludes the current position when passed to `search_with_history`, which adds it internally.
- Keep UCI stdout protocol-clean. Diagnostic handling uses `info ...`; flush after each command response.
