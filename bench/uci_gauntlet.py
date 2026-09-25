#!/usr/bin/env python3
"""Reproducible, dependency-free UCI gauntlet runner for Rookery."""
from __future__ import annotations

import argparse
import json
import queue
import re
import shlex
import subprocess
import sys
import threading
import time
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

UCI_MOVE = re.compile(r"^[a-h][1-8][a-h][1-8][qrbn]?$")
FILES = "abcdefgh"
KNIGHT = ((1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2))
KING = tuple((a, b) for a in (-1, 0, 1) for b in (-1, 0, 1) if a or b)

class GauntletError(RuntimeError): pass
class ProtocolError(GauntletError): pass
class EngineTimeout(GauntletError): pass

@dataclass(frozen=True)
class Opening:
    id: str
    fen: str

@dataclass(frozen=True)
class PairGame:
    opening: Opening
    pair: int
    rookery_white: bool


def parse_bestmove(line: str) -> str:
    fields = line.split()
    if len(fields) < 2 or fields[0] != "bestmove" or not UCI_MOVE.fullmatch(fields[1]) and fields[1] != "0000":
        raise ProtocolError("malformed bestmove line: " + line)
    return fields[1]


def color_paired_games(openings: Iterable[Opening], pairs: int) -> list[PairGame]:
    if pairs < 1: raise ValueError("pairs must be at least one")
    return [PairGame(opening, pair, white) for opening in openings for pair in range(1, pairs + 1) for white in (True, False)]


def score_games(records: Iterable[dict]) -> dict:
    totals = {"games": 0, "wins": 0, "draws": 0, "losses": 0, "score": 0.0}
    for record in records:
        if record.get("status") != "completed": continue
        totals["games"] += 1
        result, white = record["result"], record["rookery_white"]
        rookery_result = result if white else {"1-0": "0-1", "0-1": "1-0", "1/2-1/2": "1/2-1/2"}[result]
        if rookery_result == "1-0": totals["wins"] += 1; totals["score"] += 1.0
        elif rookery_result == "0-1": totals["losses"] += 1
        else: totals["draws"] += 1; totals["score"] += 0.5
    totals["score_percent"] = round(100 * totals["score"] / totals["games"], 2) if totals["games"] else 0.0
    return totals


def load_openings(path: Path, limit: int | None) -> list[Opening]:
    try: data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error: raise GauntletError(f"cannot load opening suite {path}: {error}") from error
    rows = data.get("openings") if isinstance(data, dict) else None
    if not isinstance(rows, list) or not rows: raise GauntletError("opening suite needs a non-empty 'openings' list")
    openings = []
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("id"), str) or not isinstance(row.get("fen"), str):
            raise GauntletError("every opening needs string id and fen")
        Position.from_fen(row["fen"])
        openings.append(Opening(row["id"], row["fen"]))
    if len({o.id for o in openings}) != len(openings): raise GauntletError("opening ids must be unique")
    return openings[:limit] if limit else openings

class UciEngine:
    def __init__(self, label: str, command: str, timeout: float):
        self.label, self.timeout = label, timeout
        self.process: subprocess.Popen[str] | None = None
        self.lines: queue.Queue[str | None] = queue.Queue(maxsize=512)
        self.stderr: list[str] = []
        self.reader_error: str | None = None
        self.stdout_closed = False
        try:
            self.process = subprocess.Popen(shlex.split(command), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
        except (OSError, ValueError) as error:
            raise GauntletError(f"cannot start {label}: {error}") from error
        try:
            if self.process.stdin is None or self.process.stdout is None or self.process.stderr is None:
                raise GauntletError(f"cannot pipe {label}")
            threading.Thread(target=self._read_stdout, daemon=True).start()
            threading.Thread(target=self._read_stderr, daemon=True).start()
            self.send("uci"); self.wait_for(lambda line: line == "uciok", "uciok")
            self.send("isready"); self.wait_for(lambda line: line == "readyok", "readyok")
        except BaseException:
            self.close()
            raise

    def _read_stdout(self) -> None:
        assert self.process and self.process.stdout
        try:
            for raw in self.process.stdout:
                line = raw.rstrip("\r\n")
                if self.reader_error is not None:
                    continue
                try:
                    self.lines.put_nowait(line)
                except queue.Full:
                    # Keep draining the pipe after recording the protocol error so a
                    # noisy child cannot remain blocked while close() reaps it.
                    self.reader_error = "stdout queue overflow"
        finally:
            self.stdout_closed = True
            try: self.lines.put_nowait(None)
            except queue.Full: pass

    def _read_stderr(self) -> None:
        assert self.process and self.process.stderr
        for raw in self.process.stderr:
            self.stderr.append(raw.rstrip())
            if len(self.stderr) > 40: self.stderr.pop(0)

    def send(self, command: str) -> None:
        if self.reader_error: raise ProtocolError(f"{self.label}: {self.reader_error}")
        assert self.process
        if self.process.poll() is not None: raise GauntletError(f"{self.label} exited with {self.process.returncode}; stderr: {' | '.join(self.stderr)}")
        try:
            assert self.process.stdin; self.process.stdin.write(command + "\n"); self.process.stdin.flush()
        except (BrokenPipeError, OSError) as error: raise GauntletError(f"cannot write to {self.label}: {error}") from error

    def wait_for(self, predicate, expected: str, timeout: float | None = None) -> str:
        deadline = time.monotonic() + (self.timeout if timeout is None else timeout)
        while True:
            if self.reader_error: raise ProtocolError(f"{self.label}: {self.reader_error}")
            remaining = deadline - time.monotonic()
            if remaining <= 0: raise EngineTimeout(f"{self.label}: timed out waiting for {expected}")
            try: line = self.lines.get(timeout=remaining)
            except queue.Empty:
                if self.reader_error: raise ProtocolError(f"{self.label}: {self.reader_error}")
                if self.stdout_closed: raise ProtocolError(f"{self.label} closed stdout; stderr: {' | '.join(self.stderr)}")
                raise EngineTimeout(f"{self.label}: timed out waiting for {expected}")
            if line is None or self.stdout_closed and self.lines.empty():
                raise ProtocolError(f"{self.label} closed stdout; stderr: {' | '.join(self.stderr)}")
            if predicate(line): return line

    def bestmove(self, fen: str, moves: list[str], go: str, timeout: float) -> str:
        self.send("position fen " + fen + (" moves " + " ".join(moves) if moves else ""))
        self.send(go)
        return parse_bestmove(self.wait_for(lambda line: line.startswith("bestmove"), "bestmove", timeout))

    def close(self) -> None:
        if self.process is None:
            return
        if self.process.poll() is None:
            try:
                if self.process.stdin:
                    self.process.stdin.write("quit\n")
                    self.process.stdin.flush()
            except (BrokenPipeError, OSError):
                pass
            try: self.process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            if stream:
                try: stream.close()
                except OSError: pass

# Minimal legal-move implementation: the harness, rather than an engine, adjudicates games.
def sq(name: str) -> int: return FILES.index(name[0]) + 8 * (int(name[1]) - 1)
def name(square: int) -> str: return FILES[square % 8] + str(square // 8 + 1)
def inside(x: int, y: int) -> bool: return 0 <= x < 8 and 0 <= y < 8

@dataclass
class Position:
    board: list[str]
    turn: str
    castling: str
    ep: int | None
    halfmove: int
    fullmove: int

    @classmethod
    def from_fen(cls, fen: str) -> "Position":
        fields = fen.split()
        if len(fields) != 6: raise GauntletError("FEN needs six fields")
        rows = fields[0].split("/")
        if len(rows) != 8 or fields[1] not in ("w", "b"): raise GauntletError("invalid FEN")
        board = ["."] * 64
        for display_rank, row in enumerate(rows):
            file = 0
            for char in row:
                if char.isdigit(): file += int(char)
                elif char in "PNBRQKpnbrqk" and file < 8: board[(7-display_rank)*8+file] = char; file += 1
                else: raise GauntletError("invalid FEN board")
            if file != 8: raise GauntletError("invalid FEN rank")
        try: ep = None if fields[3] == "-" else sq(fields[3]); half, full = int(fields[4]), int(fields[5])
        except (ValueError, IndexError): raise GauntletError("invalid FEN counters or en passant")
        if board.count("K") != 1 or board.count("k") != 1 or half < 0 or full < 1:
            raise GauntletError("FEN needs one king of each color and valid counters")
        return cls(board, fields[1], "" if fields[2] == "-" else fields[2], ep, half, full)

    def fen(self) -> str:
        rows = []
        for rank in range(7, -1, -1):
            empty, row = 0, ""
            for piece in self.board[rank*8:rank*8+8]:
                if piece == ".": empty += 1
                else:
                    if empty: row += str(empty); empty = 0
                    row += piece
            rows.append(row + (str(empty) if empty else ""))
        return "/".join(rows) + f" {self.turn} {self.castling or '-'} {name(self.ep) if self.ep is not None else '-'} {self.halfmove} {self.fullmove}"

    def color(self, piece: str) -> str | None: return "w" if piece.isupper() else "b" if piece != "." else None
    def attacked(self, target: int, by: str) -> bool:
        tx, ty = target % 8, target // 8
        pawn, knight, bishop, rook, queen, king = (("P", "N", "B", "R", "Q", "K") if by == "w" else ("p", "n", "b", "r", "q", "k"))
        direction = 1 if by == "w" else -1
        for dx in (-1, 1):
            x, y = tx-dx, ty-direction
            if inside(x,y) and self.board[y*8+x] == pawn: return True
        for dx,dy in KNIGHT:
            x,y=tx-dx,ty-dy
            if inside(x,y) and self.board[y*8+x] == knight: return True
        for dx,dy, pieces in ((1,0,rook+queen),(-1,0,rook+queen),(0,1,rook+queen),(0,-1,rook+queen),(1,1,bishop+queen),(1,-1,bishop+queen),(-1,1,bishop+queen),(-1,-1,bishop+queen)):
            x,y=tx+dx,ty+dy
            while inside(x,y):
                p=self.board[y*8+x]
                if p != ".":
                    if p in pieces: return True
                    break
                x,y=x+dx,y+dy
        for dx,dy in KING:
            x,y=tx+dx,ty+dy
            if inside(x,y) and self.board[y*8+x] == king: return True
        return False

    def pseudo_moves(self) -> list[str]:
        us, moves = self.turn, []
        for source,piece in enumerate(self.board):
            if self.color(piece) != us: continue
            x,y=source%8,source//8; lower=piece.lower()
            def add(dx,dy,slide=False):
                xx,yy=x+dx,y+dy
                while inside(xx,yy):
                    target=yy*8+xx
                    if self.color(self.board[target]) == us: break
                    moves.append(name(source)+name(target))
                    if self.board[target] != "." or not slide: break
                    xx,yy=xx+dx,yy+dy
            if lower == "p":
                d=1 if us=="w" else -1; start=1 if us=="w" else 6; promotion=7 if us=="w" else 0
                if inside(x,y+d) and self.board[(y+d)*8+x]==".":
                    target=(y+d)*8+x
                    moves.extend(name(source)+name(target)+q for q in "qrbn") if y+d==promotion else moves.append(name(source)+name(target))
                    if y==start and self.board[(y+2*d)*8+x]==".": moves.append(name(source)+name((y+2*d)*8+x))
                for dx in (-1,1):
                    if inside(x+dx,y+d):
                        target=(y+d)*8+x+dx
                        if self.color(self.board[target]) not in (None,us) or target==self.ep:
                            moves.extend(name(source)+name(target)+q for q in "qrbn") if y+d==promotion else moves.append(name(source)+name(target))
            elif lower == "n":
                for dx,dy in KNIGHT: add(dx,dy)
            elif lower in "brq":
                directions=[]
                if lower in "rq": directions += [(1,0),(-1,0),(0,1),(0,-1)]
                if lower in "bq": directions += [(1,1),(1,-1),(-1,1),(-1,-1)]
                for dx,dy in directions: add(dx,dy,True)
            else:
                for dx,dy in KING: add(dx,dy)
                if us=="w" and source==4 and piece=="K":
                    if "K" in self.castling and self.board[7]=="R" and self.board[5]==self.board[6]=="." and not self.attacked(4,"b") and not self.attacked(5,"b"): moves.append("e1g1")
                    if "Q" in self.castling and self.board[0]=="R" and self.board[1]==self.board[2]==self.board[3]=="." and not self.attacked(4,"b") and not self.attacked(3,"b"): moves.append("e1c1")
                if us=="b" and source==60 and piece=="k":
                    if "k" in self.castling and self.board[63]=="r" and self.board[61]==self.board[62]=="." and not self.attacked(60,"w") and not self.attacked(61,"w"): moves.append("e8g8")
                    if "q" in self.castling and self.board[56]=="r" and self.board[57]==self.board[58]==self.board[59]=="." and not self.attacked(60,"w") and not self.attacked(59,"w"): moves.append("e8c8")
        return moves

    def apply(self, move: str) -> "Position":
        source,target=sq(move[:2]),sq(move[2:4]); board=self.board.copy(); piece=board[source]; captured=board[target]
        board[source]="."
        if piece.lower()=="p" and target==self.ep and captured==".": board[target + (-8 if self.turn=="w" else 8)]="."
        board[target] = (move[4].upper() if piece.isupper() else move[4]) if len(move)==5 else piece
        if piece.lower()=="k" and abs(target-source)==2:
            rook_from,rook_to=(source+3,source+1) if target>source else (source-4,source-1)
            board[rook_to]=board[rook_from]; board[rook_from]="."
        rights=self.castling
        for marker,square in (("K",4),("Q",4),("k",60),("q",60),("Q",0),("K",7),("q",56),("k",63)):
            if source==square or target==square: rights=rights.replace(marker,"")
        ep=(source+target)//2 if piece.lower()=="p" and abs(target-source)==16 else None
        return Position(board, "b" if self.turn=="w" else "w", rights, ep, 0 if piece.lower()=="p" or captured!="." else self.halfmove+1, self.fullmove+(self.turn=="b"))

    def legal_moves(self) -> list[str]:
        enemy="b" if self.turn=="w" else "w"; legal=[]
        for move in self.pseudo_moves():
            next_position=self.apply(move)
            king = "K" if self.turn=="w" else "k"
            if king in next_position.board and not next_position.attacked(next_position.board.index(king), enemy): legal.append(move)
        return legal

def repetition_identity(position: Position) -> tuple[str, str, str, str]:
    """FIDE repetition identity, with EP only when a legal EP capture exists."""
    ep = "-"
    if position.ep is not None:
        target = position.ep
        captured = target + (-8 if position.turn == "w" else 8)
        enemy_pawn = "p" if position.turn == "w" else "P"
        if position.board[target] == "." and 0 <= captured < 64 and position.board[captured] == enemy_pawn:
            target_name = name(target)
            for move in position.legal_moves():
                source = sq(move[:2])
                if move[2:4] == target_name and position.board[source].lower() == "p":
                    ep = target_name
                    break
    return (position.fen().split()[0], position.turn, position.castling or "-", ep)


def play_game(game: PairGame, rookery: UciEngine, opponent: UciEngine, go: str, response_timeout: float, max_plies: int) -> dict:
    position=Position.from_fen(game.opening.fen); moves=[]; seen=Counter()
    while len(moves) < max_plies:
        legal=position.legal_moves()
        if not legal:
            king = "K" if position.turn == "w" else "k"
            enemy = "b" if position.turn == "w" else "w"
            if position.attacked(position.board.index(king), enemy):
                result = "0-1" if position.turn == "w" else "1-0"
                termination = "checkmate"
            else:
                result = "1/2-1/2"
                termination = "stalemate"
            return game_record(game,moves,result,termination)
        key=repetition_identity(position); seen[key]+=1
        if seen[key]>=3: return game_record(game,moves,"1/2-1/2","threefold repetition")
        if position.halfmove>=100: return game_record(game,moves,"1/2-1/2","fifty-move rule")
        engine=rookery if (position.turn=="w") == game.rookery_white else opponent
        move=engine.bestmove(game.opening.fen,moves,go,response_timeout)
        if move not in legal: raise ProtocolError(f"{engine.label}: illegal bestmove {move} in {position.fen()}")
        moves.append(move); position=position.apply(move)
    return game_record(game,moves,"1/2-1/2",f"max plies ({max_plies})")

def game_record(game: PairGame, moves: list[str], result: str, termination: str) -> dict:
    return {"status":"completed","opening":game.opening.id,"pair":game.pair,"rookery_white":game.rookery_white,"initial_fen":game.opening.fen,"moves":moves,"result":result,"termination":termination,"pgn":f'[Event "Rookery v2 gauntlet"]\n[Opening "{game.opening.id}"]\n[Result "{result}"]\n\n' + " ".join(moves) + " " + result}

def main(argv: list[str] | None = None) -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rookery", required=True, help="command for the Rookery UCI binary")
    parser.add_argument("--opponent", required=True, help="command for Stockfish or another UCI binary")
    parser.add_argument("--openings", type=Path, default=Path("bench/openings/v2-small.json"))
    parser.add_argument("--opening-limit", type=int)
    parser.add_argument("--pairs", type=int, default=1, help="color-paired games per opening")
    control=parser.add_mutually_exclusive_group(required=True); control.add_argument("--movetime-ms", type=int); control.add_argument("--depth", type=int)
    parser.add_argument("--max-plies", type=int, default=300); parser.add_argument("--startup-timeout-ms", type=int, default=5000); parser.add_argument("--response-timeout-ms", type=int)
    parser.add_argument("--output-dir", type=Path, required=True)
    args=parser.parse_args(argv)
    if args.pairs < 1 or args.movetime_ms is not None and args.movetime_ms < 1 or args.depth is not None and args.depth < 1 or args.max_plies < 1: parser.error("pairs, controls, and max plies must be positive")
    if args.output_dir.exists() and any(args.output_dir.iterdir()): parser.error("output directory must be empty or absent")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    openings=load_openings(args.openings,args.opening_limit); games=color_paired_games(openings,args.pairs)
    go=f"go movetime {args.movetime_ms}" if args.movetime_ms is not None else f"go depth {args.depth}"
    response=(args.response_timeout_ms / 1000 if args.response_timeout_ms else max(args.movetime_ms / 1000 + 2 if args.movetime_ms else 10, 2))
    config={"rookery":args.rookery,"opponent":args.opponent,"openings":str(args.openings),"opening_ids":[o.id for o in openings],"pairs":args.pairs,"go":go,"max_plies":args.max_plies,"response_timeout_seconds":response}
    records=[]; error=None; r=o=None; current=None
    try:
        r=UciEngine("rookery",args.rookery,args.startup_timeout_ms/1000); o=UciEngine("opponent",args.opponent,args.startup_timeout_ms/1000)
        for game in games:
            current=game
            r.send("ucinewgame"); r.send("isready"); r.wait_for(lambda line:line=="readyok","readyok")
            o.send("ucinewgame"); o.send("isready"); o.wait_for(lambda line:line=="readyok","readyok")
            record=play_game(game,r,o,go,response,args.max_plies); records.append(record); current=None; print(f"{game.opening.id} pair {game.pair} {'white' if game.rookery_white else 'black'}: {record['result']} ({record['termination']})",flush=True)
    except GauntletError as exc:
        error=str(exc)
        if current:
            records.append({"status":"error","opening":current.opening.id,"pair":current.pair,"rookery_white":current.rookery_white,"initial_fen":current.opening.fen,"error":error})
        print("gauntlet error: "+error,file=sys.stderr)
    finally:
        if r: r.close()
        if o: o.close()
    with (args.output_dir/"games.jsonl").open("w",encoding="utf-8") as output:
        for record in records: output.write(json.dumps(record,sort_keys=True)+"\n")
    report={"format":"rookery-v2-gauntlet-v1","config":config,"aggregate":score_games(records),"completed_games":sum(record.get("status") == "completed" for record in records),"scheduled_games":len(games),"error":error}
    (args.output_dir/"report.json").write_text(json.dumps(report,sort_keys=True,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(report["aggregate"],sort_keys=True))
    return 1 if error else 0

if __name__ == "__main__": sys.exit(main())
