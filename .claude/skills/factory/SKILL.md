---
name: factory
description: Run a feature through the AegisTARA software factory, one agent at a time (plan, build a small slice, script gate, one review, owner review, merge, learn). Use for /factory <feature>, /factory next, /factory plan <job-id>, /factory approve <slice-id>, /factory rework <slice-id> <note>, /factory resume <slice-id>, /factory retro.
argument-hint: <feature> | next | plan <job-id> | approve <slice-id> | rework <slice-id> <note> | resume <slice-id> | retro
disable-model-invocation: true
---

# /factory

You are the **orchestrator**. You move files and start agents. You never write code, specs,
prompts or reviews yourself, and you are the only writer of `factory/board.json` and
`factory/lessons.md`.

Arguments: `$ARGUMENTS`

## Three rules

1. **One agent at a time.** Never start two agents in one message. Never start an agent in
   the background. Wait for each to finish before the next step.
2. **Scripts before agents.** A check a script can do is never given to an agent. No reviewer
   runs until `factory/verify.py` passes.
3. **Agents get context only from files.** A spawn prompt is the job id, the slice id, the one
   file to follow and the one file to write. Never paste conversation context into it.

## The loop

```
backlog line
   │
 planner ──► spec.md + plan.json + prompts/<slice>.md        (open questions → owner)
   │
   ▼  for each slice, in order
 builder ──► one commit + build.md
   │
 verify.py ─► gate.md        FAIL → builder once more → still FAIL → owner
   │ PASS
 reviewer ─► review.md       CHANGES → builder once more → gate → reviewer → still CHANGES → owner
   │ PASS
 OWNER reads review.md and the diff    /factory approve | /factory rework
   │ approved
 learn ────► lessons.md, board.json
   │
 next slice … last slice approved → merge
```

Each agent ends its file with `NEXT: <role>`. Follow it when it names `builder`, `reviewer`,
`planner` or `human`, within the limits above. This is how an agent hands work on: it says
who is next, you start them.

## Files

```
factory/board.json                          you write it after every state change
factory/backlog.md                          "- [ ] feature" lines
factory/lessons.md                          what the factory learned; you write it
factory/verify.py                           the script gate
factory/jobs/<job-id>/spec.md               planner
factory/jobs/<job-id>/plan.json             planner: the slices
factory/jobs/<job-id>/prompts/<slice>.md    planner: one self-contained build prompt per slice
factory/jobs/<job-id>/slices/<slice>/build.md    builder
factory/jobs/<job-id>/slices/<slice>/gate.md     verify.py
factory/jobs/<job-id>/slices/<slice>/review.md   reviewer; also the owner's review packet
factory/jobs/<job-id>/rework.md             you: the owner's answers and rework notes
factory/worktrees/<job-id>/                 the job's git worktree (gitignored)
```

## Which model runs what

Pass `model` on every spawn; it overrides the agent file.

| Agent | Slice `tier` or `risk` | Model |
|---|---|---|
| planner | any | fable |
| builder | `small` or `standard` | sonnet |
| builder | `hard` | opus |
| reviewer | `risk: normal` | opus |
| reviewer | `risk: high` | fable |

- A builder that fails the gate twice on `small` or `standard` is not retried on a bigger
  model automatically. Stop and tell the owner; they may say `/factory rework <slice> use opus`.
- **Outside models** (DeepSeek, OpenRouter or any other): a build prompt is a complete task
  on its own, so the owner can run `factory/jobs/<job-id>/prompts/<slice>.md` in any other
  session or tool, inside `factory/worktrees/<job-id>/`, and then run `/factory resume <slice>`.
  You never send a prompt to an outside service yourself. Only `small` and `standard` slices
  with `risk: normal` may be built outside; the reviewer for those is always opus.

## board.json

One entry per job, as before. `reviews` holds the current slice, `history` the finished ones.

```json
{ "jobs": [ {
  "id": "162-workspace-files", "title": "Workspace files", "branch": "factory/162-workspace-files",
  "stage": "review", "slice": "162b", "base": "<sha before this slice>",
  "reviews": { "slice": "162b", "gate": "PASS", "review": "pending", "owner": "pending" },
  "history": [ { "slice": "162a", "gate": "PASS", "review": "PASS", "owner": "PASS",
                 "model": "sonnet", "gate_tries": 1, "review_tries": 1, "lines": 212 } ],
  "decision": null, "updated": "2026-11-10T12:00:00Z"
} ] }
```
- `stage`: `spec` (planning) → `build` → `review` (gate and reviewer) → `approve` (waiting for
  the owner) → `merged` | `needs-human`.
- Values: `pending` | `PASS` | `CHANGES` | `FAIL`.
- Rewrite the whole file each time (read, change one job, write). Update `updated`.

## Commands

### `/factory <feature>` and `/factory next`
`next` takes the first `- [ ]` line of `factory/backlog.md`. Job id: the planned id at the end
of the line `(NNN-slug)` if it has one, else the next number after the highest in board.json
plus a short slug. Then run **Plan**, then **Slice** for the first slice, and stop at the
owner gate. When the job is `merged`, tick the backlog line.

### `/factory plan <job-id>`
Run **Plan** only. Use it to get the prompts without building, for example to build outside.

### `/factory approve <slice-id>`
Only when that slice is at the owner gate. Set `owner: PASS`, run **Learn**, then **Slice**
for the next slice, or **Merge** after the last one.

### `/factory rework <slice-id> <note>`
Append `## Rework <slice-id>` and the note to `rework.md`. Set `owner: CHANGES`. Run **Learn**
(an owner rework is always a lesson). Start the builder on the same slice with the note, then
the gate and the reviewer again.

### `/factory resume <slice-id>`
The slice was built outside the factory. Check the worktree has a new commit after `base`,
then continue **Slice** from the gate.

### `/factory retro`
Read `factory/lessons.md` and the `history` of the last ten jobs. Report: slices per job, first
try gate rate, first try review rate, owner rework rate, median lines per slice, by model.
For each lesson marked `PROMOTE`, propose one concrete change as a diff: a new check in
`factory/verify.py`, a line in an agent file, or a rule in a prompt template. **Show the diff
and wait.** Apply it only when the owner says yes, then delete the lesson: it is now enforced.

## Steps

**Plan.** Add the job with `stage: spec`. If `plan.json` and a prompt for every slice already
exist, skip the planner. Otherwise spawn `planner`: "Job `<job-id>`. Feature: `<text>`.
Write `spec.md` (if missing), `plan.json` and `prompts/` in `factory/jobs/<job-id>/`."
Check that `plan.json` parses and every slice has its prompt file. Open questions in the
spec: ask the owner now (AskUserQuestion, the planner's recommendation first), write the
answers to `rework.md` under `## Answers`, and have the planner update the prompts once.

**Slice.**
1. First slice only: `git worktree add factory/worktrees/<job-id> -b factory/<job-id> HEAD`
   (the branch the main checkout is on; the job merges back into it).
2. Record `base` = `git -C factory/worktrees/<job-id> rev-parse HEAD`. Set `stage: build`.
3. Spawn `builder` with the model from the table: "Job `<job-id>`, slice `<slice-id>`. Follow
   `factory/jobs/<job-id>/prompts/<slice-id>.md`. Write `slices/<slice-id>/build.md`."
4. Gate: `python3 factory/verify.py <job-id> <slice-id> <base>`. Read the first line of
   `gate.md`.
   - `GATE: FAIL`, first time: spawn the builder again: "Slice `<slice-id>` failed the gate.
     Read `slices/<slice-id>/gate.md`, fix, commit." Run the gate again.
   - `GATE: FAIL`, second time: `needs-human`. Stop.
5. Set `stage: review`. Spawn `reviewer` with the model from the table: "Job `<job-id>`,
   slice `<slice-id>`, base `<base>`. Write `slices/<slice-id>/review.md`."
   - `VERDICT: CHANGES` with `NEXT: builder`, first time: builder again with `review.md`,
     then the gate, then the reviewer once more.
   - `VERDICT: CHANGES` a second time, or `NEXT: planner`, or `NEXT: human`: `needs-human`.
     Stop and say what the file asks.
6. `VERDICT: PASS`: set `stage: approve`. **Stop.** Tell the owner, in five lines or fewer:
   the slice id and title, the changed lines, the "For the owner" part of `review.md`, and
   the two commands `/factory approve <slice-id>` and `/factory rework <slice-id> <note>`.

**Learn.** Run after the owner decides on a slice.
- Push the slice into `history` with `model`, `gate_tries`, `review_tries` and `lines`.
- A lesson is due when the gate failed, the reviewer said CHANGES, or the owner asked for a
  rework. Take the reviewer's `## Lesson` line or the owner's note. Write the cause, not the
  symptom, in one line. If `lessons.md` already has it, raise its `seen` count instead; at
  `seen: 3` add `PROMOTE`.
- Keep `lessons.md` at 25 lessons or fewer: drop the oldest with `seen: 1`.
- Agents never edit `lessons.md`, their own agent files or `verify.py`. Only you write
  lessons, and only the owner approves a change to the factory itself.

**Merge.** After the last slice is approved:
```
git -C factory/worktrees/<job-id> status --porcelain     # must be empty
git merge --no-ff factory/<job-id> -m "Merge factory/<job-id>: <title>"
git worktree remove factory/worktrees/<job-id>
cargo clean --workspace     # tests built in the worktree point at its removed fixture paths
git add factory/jobs/<job-id> factory/board.json factory/backlog.md factory/lessons.md
git commit -m "Record factory job <job-id>"
```
Merge conflict: `git merge --abort`, `needs-human`, stop. Never push; the owner pushes and
opens the pull request.

## Done

End every command with one line: job id, slice id, stage, and the next command.
Dashboard: `cd factory && python3 -m http.server 8000` → http://localhost:8000/dashboard.html
