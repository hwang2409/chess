# Rookery v0 correctness iteration

Each feature is developed in its own Git worktree/branch, reviewed independently, and merged only after review findings are resolved.

## Feature 1: Perft regression suite (`feature/perft-regressions`)

Contract: add integration tests only; do not change engine behavior. Cover at least two established standard perft positions beyond existing start-position/Kiwipete coverage, use trustworthy FEN/count fixtures through conservative depths, and assert `Position` is unchanged after each count.

Acceptance: `cargo fmt --check` and `cargo test` pass; an independent review confirms fixture accuracy and useful coverage.

## Feature 2: Make/unmake sequence coverage (`feature/make-unmake-tests`)

Contract: add deterministic integration tests only. Walk legal move sequences from start position and special-rule positions, retain every `Undo`, unwind, and compare the complete `Position` with its initial value. Exercise promotion, en passant, and castling where practical; no external dependencies.

Acceptance: `cargo fmt --check` and `cargo test` pass; review confirms deterministic legal paths and complete restoration assertions.

## Feature 3: Game-history threefold repetition (`feature/repetition-history`)

Contract: add a backward-compatible search entry point accepting repetition-key history; detect a draw only on the third occurrence (including current position); preserve root/search history semantics; have UCI position parsing reset history for a new position and append position keys after each move, then pass history to search. Do not broaden scope to insufficient material or en-passant-key semantics.

Acceptance: tests distinguish one prior occurrence from two prior occurrences at root and verify UCI game history reaches search; `cargo fmt --check`, tests, and clippy pass; independent review is clean before merge.

## Integration policy

Branches are isolated worktrees rooted at the published `main` baseline. Review each branch before integration, address findings on that branch, then merge in sequence and run the full verification suite on `main`. Public repository: https://github.com/hwang2409/rookery-chess
