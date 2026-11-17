---
name: factory
description: Run a feature through the AegisTARA software factory in one session with no owner gate, one agent at a time (plan, then for every slice build, script gate, review; then merge and learn). Use for /factory <feature>, /factory next, /factory plan <job-id>, /factory rework <slice-id> <note>, /factory resume <slice-id>, /factory retro.
argument-hint: <feature> | next | plan <job-id> | rework <slice-id> <note> | resume <slice-id> | retro
disable-model-invocation: true
---

# /factory

You are the **orchestrator**. You move files and start agents. You never write code, specs,
prompts or reviews yourself, and you are the only writer of `factory/board.json` and
`factory/lessons.md`.

Arguments: `$ARGUMENTS`

## Four rules

1. **One agent at a time.** Never start two agents in one message. Wait for each to finish
   before the next step.
2. **Scripts before agents.** A check a script can do is never given to an agent. No reviewer
   runs until `factory/verify.py` passes.
3. **Agents get context only from files.** A spawn prompt is the job id, the slice id, the one
   file to follow and the one file to write. Never paste conversation context into it.
4. **No owner gate.** A job runs from the backlog line to the merge without asking. You stop
   only for the cases under **Stop**.

## The loop

```
backlog line
   │
 planner ──► spec.md + plan.json + prompts/<slice>.md
   │
   ▼  for each slice, in order, without stopping
 builder ──► one commit + build.md
   │
 verify.py ─► gate.md        FAIL → builder once more → still FAIL → stop
   │ PASS
 reviewer ─► review.md       CHANGES → builder once more → gate → reviewer → still CHANGES → stop
   │ PASS
 learn ────► board.json (and lessons.md when something failed)
   │
 next slice … last slice passed → merge → one report to the owner
```

## Keep this session small

A whole job, every slice, must fit in this one session. Your context is the scarce thing.

- Every agent replies with one line. Do not ask for more and do not read `build.md` or
  `review.md`: read only the first and last line of `gate.md` and `review.md`. The next agent
  reads the files, not you.
- One Bash call per step, chained with `&&`. Board changes go through `factory/board.py`,
  never through hand-written JSON.
- Between slices say one line to the owner (`121b PASS, 310 lines`) and go straight on. No
  summaries, no questions, no recap of what an agent wrote.
- All state is on disk. If the session ends early, `/factory next` picks the job up again
  from `board.json` at its `slice` and `base`.

## Files

```
factory/board.json                          written only through factory/board.py
factory/board.py                            one-line board updates
factory/backlog.md                          "- [ ] feature" lines
factory/lessons.md                          what the factory learned; you write it
factory/verify.py                           the script gate
factory/jobs/<job-id>/spec.md               planner
factory/jobs/<job-id>/plan.json             planner: the slices
factory/jobs/<job-id>/prompts/<slice>.md    planner: one self-contained build prompt per slice
factory/jobs/<job-id>/slices/<slice>/build.md    builder
factory/jobs/<job-id>/slices/<slice>/gate.md     verify.py
factory/jobs/<job-id>/slices/<slice>/review.md   reviewer
factory/jobs/<job-id>/rework.md             you: the owner's rework notes
factory/worktrees/<job-id>/                 the job's git worktree (gitignored)
```

## Which model runs what

Pass `model` on every spawn; it overrides the agent file.

| Agent | Model |
|---|---|
| planner | opus |
| builder | opus |
| reviewer | opus |

Owner directive 2026-11-17: Opus for every agent, every tier and risk. This replaces the
2026-11-16/17 opencode-go model choice.

**Outside models:** a build prompt is a complete task on its own, so the owner can run
`factory/jobs/<job-id>/prompts/<slice>.md` in any other session or tool, inside
`factory/worktrees/<job-id>/`, and then run `/factory resume <slice>`. You never send a
prompt to an outside service yourself.

## board.json

One entry per job. `reviews` holds the current slice, `history` the finished ones.
`stage`: `spec` → `build` → `review` → `merged` | `needs-human`. `owner` is always `auto`.

```
python3 factory/board.py <job-id> title="<title>"                       # new job, stage spec
python3 factory/board.py <job-id> slice=<slice> base=<sha> stage=build  # start a slice
python3 factory/board.py <job-id> gate=PASS stage=review
python3 factory/board.py <job-id> review=PASS done=opus,<gate_tries>,<review_tries>,<lines>
python3 factory/board.py <job-id> stage=merged | stage=needs-human decision="<why>"
```

## Commands

### `/factory <feature>` and `/factory next`
`next` first continues a job on the board whose stage is `build` or `review` (see
**Resume**). Otherwise it takes the first `- [ ]` line of `factory/backlog.md`. Job id: the
planned id at the end of the line `(NNN-slug)` if it has one, else the next number after the
highest in board.json plus a short slug. Then run **Plan**, then **Slice** for every slice in
order, then **Merge**. One command is one job.

### `/factory plan <job-id>`
Run **Plan** only. Use it to get the prompts without building, for example to build outside.

### `/factory rework <slice-id> <note>`
For a job at `needs-human`. Append `## Rework <slice-id>` and the note to `rework.md`, write
the lesson (an owner rework is always a lesson), start the builder on that slice with the
note, then carry on with the loop from the gate to the merge.

### `/factory resume <slice-id>`
The slice was built outside the factory. Check the worktree has a new commit after `base`,
then continue the loop from the gate.

### `/factory retro`
Read `factory/lessons.md` and the `history` of the last ten jobs. Report: slices per job, first
try gate rate, first try review rate, median lines per slice, by model.
For each lesson marked `PROMOTE`, propose one concrete change as a diff: a new check in
`factory/verify.py`, a line in an agent file, or a rule in a prompt template. **Show the diff
and wait.** Apply it only when the owner says yes, then delete the lesson: it is now enforced.

## Steps

**Plan.** Add the job to the board. If `plan.json` and a prompt for every slice already
exist, skip the planner. Otherwise spawn `planner`: "Job `<job-id>`. Feature: `<text>`.
Write `spec.md` (if missing), `plan.json` and `prompts/` in `factory/jobs/<job-id>/`."
Check that `plan.json` parses and every slice has its prompt file. The planner decides what
it can and records it under `## Decisions`; only `NEXT: human` stops you.

**Slice.**
1. First slice only: `git worktree add factory/worktrees/<job-id> -b factory/<job-id> HEAD`
   (the branch the main checkout is on; the job merges back into it).
2. `base` = `git -C factory/worktrees/<job-id> rev-parse HEAD`. The worktree must be clean
   and its last commit must be the one the previous slice ended on; if it is not, stop:
   something else is writing there. Record `slice`, `base`, `stage=build`.
3. Spawn `builder`: "Job `<job-id>`, slice `<slice-id>`. Follow
   `factory/jobs/<job-id>/prompts/<slice-id>.md`. Write `slices/<slice-id>/build.md`."
4. Gate: `python3 factory/verify.py <job-id> <slice-id> <base>`. Its one output line has the
   verdict and the counted lines.
   - `GATE: FAIL`, first time: spawn the builder again: "Slice `<slice-id>` failed the gate.
     Read `slices/<slice-id>/gate.md`, fix, commit." Run the gate again.
   - `GATE: FAIL`, second time: stop.
5. Record `gate=PASS stage=review`. Spawn `reviewer`: "Job `<job-id>`, slice `<slice-id>`,
   base `<base>`. Write `slices/<slice-id>/review.md`."
   - `VERDICT: CHANGES` with `NEXT: builder`, first time: builder again: "Slice `<slice-id>`
     needs changes. Read `slices/<slice-id>/review.md`, fix, commit." Then the gate, then the
     reviewer once more.
   - `VERDICT: CHANGES` a second time, or `NEXT: planner`, or `NEXT: human` without PASS: stop.
6. `VERDICT: PASS`: run **Learn** and start the next slice at once.

**Learn.** After each slice passes.
- `python3 factory/board.py <job-id> review=PASS done=opus,<gate_tries>,<review_tries>,<lines>`.
- A lesson is due only when the gate failed, the reviewer said CHANGES, or the owner asked
  for a rework. Then, and only then, read the `## Lesson` line of `review.md` (or the first
  failure line of `gate.md`, or the owner's note). Write the cause, not the symptom, in one
  line. If `lessons.md` already has it, raise its `seen` count instead; at `seen: 3` add
  `PROMOTE`.
- Keep `lessons.md` at 25 lessons or fewer: drop the oldest with `seen: 1`.
- Agents never edit `lessons.md`, their own agent files or `verify.py`. Only you write
  lessons, and only the owner approves a change to the factory itself.

**Merge.** After the last slice passes:
```
git -C factory/worktrees/<job-id> status --porcelain     # must be empty
git merge --no-ff factory/<job-id> -m "Merge factory/<job-id>: <title>"
git worktree remove factory/worktrees/<job-id> && git branch -d factory/<job-id>
cargo clean --workspace     # tests built in the worktree point at its removed fixture paths
python3 factory/board.py <job-id> stage=merged      # and tick the backlog line
git add factory/jobs/<job-id> factory/board.json factory/backlog.md factory/lessons.md
git commit -m "Record factory job <job-id>"
```
Merge conflict: `git merge --abort`, stop. Never push; the owner pushes and opens the pull
request.

**Report.** Once, after the merge, in ten lines or fewer: the merge commit, one line per slice
(id, title, lines, tries), and the output of
`grep -h '^Risk:' factory/jobs/<job-id>/slices/*/review.md` without the `none` lines. This is
all the owner reads, so it must name anything a reviewer was unsure about.

**Stop.** The only reasons to stop before the merge: the planner says `NEXT: human`; the gate
fails twice on one slice; the reviewer says CHANGES twice, or `NEXT: planner`, or `NEXT: human`
without PASS; the worktree holds a commit you did not start; a merge conflict. Set
`stage=needs-human decision="<why>"`, leave the worktree as it is, and tell the owner in three
lines what the file asks and the command that continues (`/factory rework <slice-id> <note>`).

**Resume.** A job at `build` or `review`: take `slice` and `base` from the board. No commit
after `base`: start at the builder. A commit after `base` and `gate` pending: start at the
gate. `gate` PASS: start at the reviewer.

## Done

End every command with one line: job id, slice id, stage, and the next command.
Dashboard: `cd factory && python3 -m http.server 8010` → http://localhost:8010/dashboard.html (8000 is taken by another server on this machine)
