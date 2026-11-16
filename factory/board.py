#!/usr/bin/env python3
"""One-line updates to factory/board.json, so the orchestrator spends no tokens on JSON.

Usage: python3 factory/board.py <job-id> key=value ...

  title=<text> stage=<stage> base=<sha> decision=<text>
  slice=<id>            start a slice: gate and review go back to pending
  gate=PASS review=PASS the current slice
  done=<model>,<gate_tries>,<review_tries>,<lines>   push the current slice into history

A job id that is not on the board is added with stage `spec`.
"""
import datetime
import json
import pathlib
import sys

BOARD = pathlib.Path(__file__).resolve().parent / "board.json"


def update(board, job, pairs):
    j = next((x for x in board["jobs"] if x["id"] == job), None)
    if j is None:
        j = {"id": job, "title": job, "branch": f"factory/{job}", "stage": "spec", "slice": None,
             "base": None, "reviews": {}, "history": [], "decision": None}
        board["jobs"].append(j)
    for pair in pairs:
        key, _, value = pair.partition("=")
        if key == "slice":
            j["slice"] = value
            j["reviews"] = {"slice": value, "gate": "pending", "review": "pending", "owner": "auto"}
        elif key in ("gate", "review"):
            j["reviews"][key] = value
        elif key == "done":
            model, gate_tries, review_tries, lines = value.split(",")
            j["history"].append({"slice": j["slice"], "gate": "PASS", "review": "PASS", "owner": "auto",
                                 "model": model, "gate_tries": int(gate_tries),
                                 "review_tries": int(review_tries), "lines": int(lines)})
        elif key in ("title", "stage", "base", "decision"):
            j[key] = value
        else:
            raise SystemExit(f"unknown key: {key}\n{__doc__}")
    j["updated"] = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    return j


def self_check():
    b = {"jobs": []}
    j = update(b, "1-x", ["title=X", "slice=1a", "base=123", "stage=build", "gate=PASS"])
    assert j["base"] == "123" and j["reviews"]["gate"] == "PASS" and j["reviews"]["review"] == "pending"
    update(b, "1-x", ["review=PASS", "done=opus,1,2,300", "slice=1b"])
    assert j["history"] == [{"slice": "1a", "gate": "PASS", "review": "PASS", "owner": "auto",
                             "model": "opus", "gate_tries": 1, "review_tries": 2, "lines": 300}]
    assert j["slice"] == "1b" and j["reviews"]["gate"] == "pending" and len(b["jobs"]) == 1
    print("board.py self-check ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        self_check()
    elif len(sys.argv) < 3:
        print(__doc__)
        sys.exit(2)
    else:
        board = json.loads(BOARD.read_text())
        job = update(board, sys.argv[1], sys.argv[2:])
        BOARD.write_text(json.dumps(board, indent=2) + "\n")
        print(f"{job['id']} {job['slice']} {job['stage']}")
