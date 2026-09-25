import json
import os
import shlex
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

from bench.uci_gauntlet import (
    EngineTimeout,
    Opening,
    PairGame,
    Position,
    ProtocolError,
    UciEngine,
    color_paired_games,
    load_openings,
    main,
    parse_bestmove,
    play_game,
    repetition_identity,
    score_games,
)


class UciGauntletTests(unittest.TestCase):
    def test_bestmove_parser_accepts_uci_and_rejects_malformed_protocol(self):
        self.assertEqual(parse_bestmove("bestmove e2e4 ponder e7e5"), "e2e4")
        self.assertEqual(parse_bestmove("bestmove 0000"), "0000")
        for line in ("bestmove", "bestmove e9e4", "info depth 1"):
            with self.assertRaises(ProtocolError):
                parse_bestmove(line)

    def test_pairing_is_stable_and_color_paired(self):
        games = color_paired_games([Opening("a", "fen-a"), Opening("b", "fen-b")], 2)
        self.assertEqual([(g.opening.id, g.pair, g.rookery_white) for g in games], [
            ("a", 1, True), ("a", 1, False), ("a", 2, True), ("a", 2, False),
            ("b", 1, True), ("b", 1, False), ("b", 2, True), ("b", 2, False),
        ])

    def test_scoring_normalizes_black_games_and_ignores_incomplete_records(self):
        records = [
            {"status": "completed", "rookery_white": True, "result": "1-0"},
            {"status": "completed", "rookery_white": False, "result": "1-0"},
            {"status": "completed", "rookery_white": False, "result": "1/2-1/2"},
            {"status": "failed", "rookery_white": True, "result": "0-1"},
        ]
        self.assertEqual(score_games(records), {"games": 3, "wins": 1, "draws": 1, "losses": 1, "score": 1.5, "score_percent": 50.0})

    def test_opening_fixture_loads_in_file_order_and_limits_deterministically(self):
        fixture = {"openings": [
            {"id": "one", "fen": "4k3/8/8/8/8/8/8/4K3 w - - 0 1"},
            {"id": "two", "fen": "4k3/8/8/8/8/8/8/4K3 b - - 0 1"},
        ]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "suite.json"
            path.write_text(json.dumps(fixture), encoding="utf-8")
            self.assertEqual([opening.id for opening in load_openings(path, 1)], ["one"])

    def test_white_and_black_stalemate_are_draws(self):
        cases = [
            ("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", False),
            ("8/8/8/8/8/6k1/5q2/7K w - - 0 1", True),
        ]
        for fen, rookery_white in cases:
            with self.subTest(fen=fen):
                game = PairGame(Opening("stalemate", fen), 1, rookery_white)
                record = play_game(game, None, None, "go depth 1", 0.1, 1)
                self.assertEqual(record["result"], "1/2-1/2")
                self.assertEqual(record["termination"], "stalemate")

    def test_repetition_identity_ignores_unavailable_and_pinned_ep_but_keeps_legal_ep(self):
        cases = [
            ("4k3/8/8/3p4/8/8/8/4K3 w - d6 0 1", False),
            ("4k3/8/8/8/2p5/8/8/4K3 b - d3 0 1", False),
            ("8/6bb/8/8/R1pP2k1/4P3/P7/K7 b - d3 0 1", False),
            ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", True),
        ]
        for with_ep, expected_ep in cases:
            without_ep = with_ep.replace(" d6 ", " - ").replace(" d3 ", " - ")
            with self.subTest(fen=with_ep):
                identity = repetition_identity(Position.from_fen(with_ep))
                self.assertEqual(identity[-1] != "-", expected_ep)
                self.assertEqual(repetition_identity(Position.from_fen(with_ep)) == repetition_identity(Position.from_fen(without_ep)), not expected_ep)

    def test_constructor_failure_reaps_fake_engine_for_all_handshake_failures(self):
        for mode, error in (("missing-uciok", EngineTimeout), ("missing-readyok", EngineTimeout), ("closed-stdout", ProtocolError)):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                pid_file = Path(directory) / "pid"
                script = Path(directory) / "fake_engine.py"
                script.write_text(_FAKE_ENGINE, encoding="utf-8")
                command = f"{shlex.quote(sys.executable)} {shlex.quote(str(script))} {mode} {shlex.quote(str(pid_file))}"
                with self.assertRaises(error):
                    UciEngine("fake", command, 0.15)
                pid = _wait_for_pid(pid_file)
                self._assert_process_reaped(pid)

    def test_stdout_queue_overflow_is_an_explicit_protocol_error(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "pid"
            script = Path(directory) / "fake_engine.py"
            script.write_text(_FAKE_ENGINE, encoding="utf-8")
            command = f"{shlex.quote(sys.executable)} {shlex.quote(str(script))} overflow {shlex.quote(str(pid_file))}"
            with self.assertRaisesRegex(ProtocolError, "stdout queue overflow"):
                UciEngine("fake", command, 1)
            self._assert_process_reaped(_wait_for_pid(pid_file))

    def test_mid_game_protocol_failure_is_not_counted_as_completed(self):
        fixture = {"openings": [{"id": "one", "fen": "4k3/8/8/8/8/8/8/4K3 w - - 0 1"}]}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            openings = root / "openings.json"
            output = root / "output"
            openings.write_text(json.dumps(fixture), encoding="utf-8")
            with patch("bench.uci_gauntlet.UciEngine", _MidGameFailureEngine):
                result = main(["--rookery", "rookery", "--opponent", "opponent", "--openings", str(openings), "--depth", "1", "--output-dir", str(output)])
            report = json.loads((output / "report.json").read_text(encoding="utf-8"))
            records = [json.loads(line) for line in (output / "games.jsonl").read_text(encoding="utf-8").splitlines()]
            self.assertEqual(result, 1)
            self.assertEqual(report["completed_games"], 0)
            self.assertEqual(records[0]["status"], "error")

    def _assert_process_reaped(self, pid):
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return
            time.sleep(0.01)
        self.fail(f"fake engine child {pid} remains after constructor failure")


class _MidGameFailureEngine:
    def __init__(self, label, command, timeout):
        self.label = label

    def send(self, command):
        pass

    def wait_for(self, predicate, expected, timeout=None):
        return "readyok"

    def bestmove(self, fen, moves, go, timeout):
        raise ProtocolError(f"{self.label}: simulated mid-game protocol failure")

    def close(self):
        pass


def _wait_for_pid(path):
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        if path.exists():
            return int(path.read_text(encoding="utf-8"))
        time.sleep(0.01)
    raise AssertionError("fake engine did not record its pid")


_FAKE_ENGINE = r'''import os
import sys
import time

mode, pid_file = sys.argv[1:]
with open(pid_file, "w", encoding="utf-8") as output:
    output.write(str(os.getpid()))
    output.flush()
if mode == "closed-stdout":
    os.close(sys.stdout.fileno())
    time.sleep(60)
for line in sys.stdin:
    command = line.strip()
    if mode == "missing-readyok" and command == "uci":
        print("uciok", flush=True)
    elif mode == "overflow" and command == "uci":
        for _ in range(1024):
            print("info string flood", flush=True)
    time.sleep(60)
'''


if __name__ == "__main__":
    unittest.main()
