import json
import tempfile
import unittest
from pathlib import Path

from bench.uci_gauntlet import Opening, ProtocolError, color_paired_games, load_openings, parse_bestmove, score_games


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


if __name__ == "__main__":
    unittest.main()
