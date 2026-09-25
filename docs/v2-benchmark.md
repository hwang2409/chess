# V2 UCI benchmark harness

`bench/uci_gauntlet.py` is a Python 3 standard-library-only UCI gauntlet runner. It starts two UCI programs, completes `uci`/`uciok` and `isready`/`readyok` handshakes, sends `ucinewgame`, and supplies each search with a legal `position fen ... moves ...` command. It is intentionally outside the engine binary and does not add a Python package dependency.

The committed `bench/openings/v2-small.json` suite is a small, fixed-order collection of FEN positions. Each opening is played as a color pair: Rookery is White, then Rookery is Black. Pairing, suite order, and repeat order are deterministic; there is no random seed or opening shuffle.

## Install and run

Only Python 3.10+ and executable UCI binaries are needed. Build Rookery first:

```sh
cargo build --release
python3 bench/uci_gauntlet.py \
  --rookery ./target/release/rookery \
  --opponent /path/to/stockfish \
  --movetime-ms 100 \
  --pairs 2 \
  --output-dir benchmark-results/v2-100ms
```

`--rookery` and `--opponent` accept shell-style command strings, so an alternate engine or its arguments can be supplied (for example `"/path/to/engine --uci"`). Use exactly one fair fixed control that both engines support:

- `--movetime-ms N` sends `go movetime N` to both engines.
- `--depth N` sends `go depth N` to both engines.

Useful workload controls are `--opening-limit N`, `--pairs N`, and `--max-plies N`. The output directory must be absent or empty, preventing accidental mixing of results. `--startup-timeout-ms` bounds handshakes and readiness; `--response-timeout-ms` bounds each `bestmove` (otherwise it is derived from movetime, or is 10 seconds for fixed depth).

A Stockfish installation is not required for tests. A short local protocol/game smoke test can instead run Rookery against itself:

```sh
cargo build --release
python3 bench/uci_gauntlet.py \
  --rookery ./target/release/rookery \
  --opponent ./target/release/rookery \
  --depth 1 --opening-limit 1 --pairs 1 --max-plies 12 \
  --output-dir /tmp/rookery-v2-smoke
```

## Records and failures

`games.jsonl` contains one JSON object for every completed game, including opening id and FEN, pair/color assignment, UCI moves, result, termination, and a PGN-like text rendering. `report.json` includes the immutable run configuration and Rookery-centric aggregate `wins`, `draws`, `losses`, score, score percentage, scheduled/completed counts, and any error.

The harness independently generates legal moves to validate each returned UCI move and adjudicate checkmate, stalemate, threefold repetition, the fifty-move rule, and the configured max-ply draw. It uses bounded stdout queues, drains stderr, verifies UCI response markers, and exits nonzero after writing a report when a process crashes, closes its pipe, overflows protocol output, times out, sends malformed `bestmove`, or returns an illegal move. Do not treat a partial report as a benchmark result.

## V2 baseline procedure

1. Start from a clean, committed Rookery revision and record `git rev-parse HEAD`, Rust version, Python version, host/OS, and opponent name/version in the benchmark notes or CI artifact.
2. Build a release binary with `cargo build --release`; use the exact binary for all games.
3. Use a fixed Stockfish binary/configuration, no opening book, no pondering, and one documented fair control. For the first comparable baseline use the committed suite, `--movetime-ms 100`, and `--pairs 10` (80 games).
4. Run the command above into a fresh output directory. Preserve both JSON files unchanged. Report `report.json` plus the recorded environment; compare only like-for-like runs.
5. Repeat the complete baseline after a search/evaluation change. Investigate protocol errors or incomplete game counts rather than comparing their score.
