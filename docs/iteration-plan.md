# Rookery v1 iteration plan and status

Rookery's v1 correctness-oriented scope is implemented. Work was developed in isolated branches, reviewed, and integrated before this release-readiness pass. The remaining work is post-v1 enhancement, not a promised v1 feature set.

## Completed v1 work

| Area | Delivered scope |
| --- | --- |
| Core position and moves | Bitboard board representation, FEN serialization/parsing, legal move generation, and reversible make/unmake, including castling, en passant, and promotion. |
| FEN validation | Structural six-field validation; board-rank and piece syntax checks; active-color, canonical castling-field, en-passant-rank, and counter validation. Parser deliberately does not prove arbitrary positions legal or reachable. |
| Perft and mutation safety | Start-position and Kiwipete checks plus standard positions and targeted pin, en-passant-discovered-check, and castling edge cases. Deterministic move-sequence tests verify complete make/unmake restoration. |
| Draw handling | Search detects checkmate/stalemate before draw adjudication, then handles threefold repetition from game/search history, the 100-halfmove threshold, and conservative insufficient-material cases. |
| Repetition identity | Repetition keys distinguish an en-passant target only when at least one legal en-passant capture exists; probing does not mutate the position. |
| Search | Iterative deepening, alpha-beta, quiescence, basic evaluation/capture ordering, time limits, and a per-search fixed-size direct-mapped transposition table whose entries are keyed by position and repetition path. |
| UCI | Handshake, readiness, positions with move lists, depth/time searches, perft/divide, and worker-thread search cancellation via `stop`. Queued position/go commands wait for the stopped search; queued quit suppresses its response. |

## Release-readiness verification

The release-readiness branch runs the dependency-free crate's formatting, lint, debug all-target test, release all-target test, and deterministic release-binary UCI smoke checks. The test suite covers library behavior and subprocess UCI behavior; it is not a proof of full rules, protocol, or playing-strength coverage.

## Deferred after v1

- Competitive-engine work: faster sliding attacks, stronger move ordering and pruning, opening books, pondering, tablebases, and persistent/trans-search TT management.
- Broader UCI support and more sophisticated clock management.
- Full game-adjudication and claimable-draw protocol behavior, broader dead-position analysis, and validation of all FEN position invariants/reachability.
- Additional long-running randomized, differential, and platform-specific testing beyond the bounded regression suite.
