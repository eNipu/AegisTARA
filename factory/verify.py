#!/usr/bin/env python3
"""The script gate for one factory slice. No agent, no tokens.

Usage: python3 factory/verify.py <job-id> <slice-id> <base-sha> [--no-ci]

Checks the slice's diff (base..HEAD in factory/worktrees/<job-id>) against
factory/jobs/<job-id>/plan.json, then runs scripts/ci.sh and the slice's own
checks. Writes factory/jobs/<job-id>/slices/<slice-id>/gate.md, whose first
line is `GATE: PASS` or `GATE: FAIL`. Exit code 0 on pass, 1 on fail.

--no-ci skips the cargo runs (for testing this script).
"""
import fnmatch
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# A slice may never change the things that judge it, unless its plan lists them.
PROTECTED = [".claude/*", "factory/verify.py", "factory/lessons.md", "scripts/*", ".github/*", "deny.toml"]
# Lines here do not count toward the review-size cap.
NOT_COUNTED = ["*/tests/*", "*/fixtures/*", "*/benches/*", "*/fuzz/*", "docs/*", "factory/*", "*.md", "Cargo.lock"]
TAIL = 60  # lines of command output kept in gate.md


def matches(path, globs):
    return any(fnmatch.fnmatch(path, g) for g in globs)


def run(cmd, cwd, shell=False):
    p = subprocess.run(cmd, cwd=cwd, shell=shell, capture_output=True, text=True)
    return p.returncode, (p.stdout + p.stderr).strip()


def check_diff(numstat, slice_):
    """Pure part: numstat text + the slice's plan -> (failures, counted lines)."""
    allow = slice_.get("allow", [])
    failures, counted = [], 0
    rows = [line.split("\t") for line in numstat.splitlines() if line.strip()]
    if not rows:
        return ["no changes since base: the builder committed nothing"], 0
    for added, removed, path in rows:
        if not matches(path, allow):
            failures.append(f"out of scope: {path} (allowed: {', '.join(allow)})")
        elif matches(path, PROTECTED) and not any(g == path for g in allow):
            failures.append(f"protected file: {path} (the plan must list it by exact path)")
        if path.endswith("Cargo.lock") and not slice_.get("new_deps", False):
            failures.append("Cargo.lock changed but the plan says new_deps is false")
        if not matches(path, NOT_COUNTED) and added.isdigit():
            counted += int(added) + int(removed)
    cap = slice_.get("max_lines", 400)
    if counted > cap:
        failures.append(f"too big to review: {counted} changed lines, cap {cap}; ask the planner to split")
    return failures, counted


def main(argv):
    if len(argv) < 4:
        print(__doc__)
        return 2
    job, slice_id, base = argv[1:4]
    no_ci = "--no-ci" in argv
    job_dir = ROOT / "factory" / "jobs" / job
    tree = ROOT / "factory" / "worktrees" / job
    if not tree.is_dir():
        tree = ROOT  # ponytail: a job built on the main checkout; worktrees are the normal case
    plan = json.loads((job_dir / "plan.json").read_text())
    slice_ = next((s for s in plan["slices"] if s["id"] == slice_id), None)
    if slice_ is None:
        print(f"slice {slice_id} is not in {job_dir / 'plan.json'}")
        return 2

    code, numstat = run(["git", "diff", "--numstat", f"{base}..HEAD"], tree)
    failures, counted = ([f"git diff failed: {numstat}"], 0) if code else check_diff(numstat, slice_)
    report = [f"Diff: {counted} counted lines, {len(numstat.splitlines())} files"]

    if not failures and not no_ci:
        commands = [(["scripts/ci.sh", *slice_.get("ci", [])], False)]
        commands += [(c, True) for c in slice_.get("checks", [])]
        for cmd, shell in commands:
            code, out = run(cmd, tree, shell)
            name = cmd if shell else " ".join(cmd)
            report.append(f"{'ok  ' if code == 0 else 'FAIL'} {name}")
            if code != 0:
                failures.append(f"command failed: {name}")
                report += ["```", *out.splitlines()[-TAIL:], "```"]

    out_dir = job_dir / "slices" / slice_id
    out_dir.mkdir(parents=True, exist_ok=True)
    verdict = "FAIL" if failures else "PASS"
    lines = [f"GATE: {verdict}", f"Slice: {slice_id}  Base: {base}", ""]
    lines += [f"- {f}" for f in failures] + [""] + report
    (out_dir / "gate.md").write_text("\n".join(lines) + "\n")
    print(f"GATE: {verdict} ({len(failures)} failures) -> {out_dir / 'gate.md'}")
    return 1 if failures else 0


def self_check():
    s = {"allow": ["crates/aegis-model/*"], "max_lines": 10, "new_deps": False}
    assert check_diff("", s)[0], "empty diff must fail"
    assert check_diff("3\t1\tcrates/aegis-model/src/a.rs", s) == ([], 4)
    assert "out of scope" in check_diff("1\t0\tcrates/aegis-server/src/a.rs", s)[0][0]
    assert "too big" in check_diff("9\t9\tcrates/aegis-model/src/a.rs", s)[0][0]
    assert check_diff("900\t0\tcrates/aegis-model/tests/t.rs", s) == ([], 0), "tests are not counted"
    wide = {"allow": ["*"], "max_lines": 10}
    assert "protected" in check_diff("1\t0\tscripts/ci.sh", wide)[0][0]
    assert "Cargo.lock" in check_diff("1\t0\tCargo.lock", wide)[0][0]
    assert check_diff("-\t-\tcrates/aegis-model/tests/fixtures/x.bin", s) == ([], 0), "binary files"
    print("verify.py self-check ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        self_check()
    else:
        sys.exit(main(sys.argv))
