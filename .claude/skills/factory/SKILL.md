---
name: factory
description: Run a feature through the AegisTARA software factory (spec, build, 4 parallel reviews, approve, merge). Use for /factory <feature>, /factory next, /factory approve <job-id>, /factory rework <job-id> <note>.
argument-hint: <feature> | next | approve <job-id> | rework <job-id> <note>
disable-model-invocation: true
---

# /factory

You are the **orchestrator**. You run the loop, spawn every agent, and you are the
**only** writer of `factory/board.json`. You never write code, specs, or reviews yourself.

Arguments: `$ARGUMENTS`

## The loop

```
feature → spec-writer → builder ◄──────────────┐
                           │                    │ any CHANGES (max 2 reworks)
          ┌────────┬───────┴─┬──────────┐       │
       security    ux     ui-design    code     │
          └────────┴────┬────┴──────────┘       │
                  WAIT FOR ALL 4 ───────────────┘
                        │ all PASS
                     approver ── APPROVE → merge to master
                              └─ ESCALATE → needs-human (/factory approve)
```

## Files

```
factory/board.json                        # you write it after every state change
factory/backlog.md                        # "- [ ] feature" lines
factory/jobs/<job-id>/spec.md             # spec-writer
factory/jobs/<job-id>/build.md            # builder (one section per round)
factory/jobs/<job-id>/round-N/review-{security,ux,ui,code}.md   # reviewers
factory/jobs/<job-id>/rework.md           # you: the owner's answers and rework notes
factory/jobs/<job-id>/decision.md         # approver
factory/worktrees/<job-id>/               # builder's git worktree (gitignored)
```

Agents get context **only from files**. Every spawn prompt is just: the job id, the
paths to read, and the one path to write. Never paste conversation context into a prompt.

## board.json

```json
{ "jobs": [ {
  "id": "003-rate-limiting", "title": "Rate limiting", "branch": "factory/003-rate-limiting",
  "stage": "review", "round": 2,
  "reviews": { "security": "CHANGES", "ux": "PASS", "ui": "pending", "code": "PASS" },
  "history": [ { "security": "CHANGES", "ux": "PASS", "ui": "PASS", "code": "CHANGES" } ],
  "decision": null, "updated": "2026-10-18T12:00:00Z"
} ] }
```

- `stage`: `spec` → `build` → `review` → `approve` → `merged` | `needs-human`.
- `reviews`: verdicts of the current round (`pending` | `PASS` | `CHANGES`).
- `history`: finished earlier rounds, oldest first. Push `reviews` here before starting a new round.
- `decision`: `null` | `APPROVE` | `ESCALATE`.
- Rewrite the whole file each time (read, change one job, write). Update `updated`.

## Commands

### `/factory <feature>`
1. Job id: if the feature text ends with a planned id `(NNN-slug)` that is not yet in board.json,
   use it (docs/requirements.md refers to jobs by these ids). Otherwise the next 3-digit number
   after the highest id in board.json, plus a short kebab slug (`004-export-csv`). Create `factory/jobs/<job-id>/`. Add the job to board.json with `stage: spec`.
2. Run **Steps 1–5** below.

### `/factory next`
Take the first `- [ ]` line in `factory/backlog.md`. Run it as `/factory <that text>`.
When the job reaches `merged` or `needs-human`, change that line to `- [x] … (<job-id>)`.
Empty backlog: say so and stop.

### `/factory approve <job-id>`
Only for a job in `needs-human`. Run **Step 6 (merge)**. Any other stage: refuse and say why.

### `/factory rework <job-id> <note>`
1. Append `## Rework <n>` + the note to `factory/jobs/<job-id>/rework.md`.
2. Set `stage: build`, reset the rework counter for this job, and resume the builder
   (Step 3, "resume" form) pointing it at `rework.md`. If the job was already merged,
   the builder starts a fresh branch from master with the same name plus `-r<n>`.
3. Continue with Step 4.

## Steps

**1. Spec.** Spawn `spec-writer`: "Job `<job-id>`. Feature: `<feature text>`. Write
`factory/jobs/<job-id>/spec.md`." When it returns, check the file exists.
If the spec has `## Open questions`, ask the user now (AskUserQuestion, the spec's default
first and marked Recommended), before any build. Write the answers to `rework.md` under
`## Answers`. The builder, reviewers, and approver all read it, so the job won't escalate
later for questions you already answered.

**2. Branch.** Create the worktree yourself (the builder works there so your checkout of
master stays untouched):
`git worktree add factory/worktrees/<job-id> -b factory/<job-id> master`.
Set `stage: build`.

**3. Build.**
- First round: spawn `builder` **with the name `builder-<job-id>`**: "Job `<job-id>`. Worktree
  `factory/worktrees/<job-id>`. Read `factory/jobs/<job-id>/spec.md`. Write `build.md`."
- Later rounds: **resume the same builder** with `SendMessage` to `builder-<job-id>`:
  "Round `<N>` reviews are in `factory/jobs/<job-id>/round-<N>/`. Address every CHANGES
  item, commit, and append a `## Round <N+1>` section to build.md."
  If that builder is gone (new session), spawn a fresh `builder-<job-id>` with the same
  message plus the first-round paths.
- Never resume the builder while a review round is still open.

**4. Review round N.** Set `stage: review`, `round: N`, all four `reviews` to `pending`.
Create `factory/jobs/<job-id>/round-N/`. Spawn all four reviewers **in one message, in
parallel**: `review-security`, `review-ux`, `review-ui`, `review-code`. Each gets: "Job
`<job-id>`, round `<N>`, branch `factory/<job-id>`. Write
`factory/jobs/<job-id>/round-<N>/review-<kind>.md`."
- As each reviewer returns, read the first line of its file and set its chip
  (`VERDICT: PASS` → `PASS`, anything else → `CHANGES`). Write board.json each time.
- **WAIT FOR ALL:** the round is done only when all four files exist in `round-N/`.
  A reviewer that returned without its file: respawn it once; still missing → `needs-human`.
- All PASS → Step 5.
- Any CHANGES and fewer than 2 reworks so far → push `reviews` to `history`, go to Step 3
  (resume) with round N, then Step 4 with round N+1.
- Any CHANGES after 2 reworks → `stage: needs-human`. Stop and tell the user.

**5. Approve.** Set `stage: approve`. Spawn `approver`: "Job `<job-id>`. Write
`factory/jobs/<job-id>/decision.md`." Read the first line:
- `APPROVE` → Step 6.
- `ESCALATE` (or anything else) → `stage: needs-human`, `decision: ESCALATE`. Stop and
  tell the user to read decision.md and run `/factory approve <job-id>` or `/factory rework`.

**6. Merge.**
```
git -C factory/worktrees/<job-id> status --porcelain     # must be empty
git merge --no-ff factory/<job-id> -m "Merge factory/<job-id>: <title>"
git worktree remove factory/worktrees/<job-id>
```
Merge conflict: `git merge --abort`, set `needs-human`, stop. On success set
`stage: merged`, then commit the job folder and board on master:
`git add factory/jobs/<job-id> factory/board.json factory/backlog.md && git commit -m "Record factory job <job-id>"`.

## Done

End every command with one line: job id, final stage, and the next command the user may want.
Dashboard: `cd factory && python3 -m http.server 8000` → http://localhost:8000/dashboard.html
