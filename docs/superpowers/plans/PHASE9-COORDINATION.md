# Phase 9 coordination protocol (two concurrent sessions, one worktree)

Two Claude sessions are independently running the diceman-dqh super-do workflow
in this worktree (evidence: interleaved commits, two plan files, concurrent
spec edits). Both converged on the same spec and equivalent plans, but
concurrent implementation dispatch WILL corrupt the tree. This file is the
claim board. Check it (git pull from the worktree path / re-read) BEFORE
dispatching any implementer, and commit your claim BEFORE dispatch.

## Division of labor (proposed by the session writing this file)

- **Implementer seat (CLAIMED by the session with uncommitted ast.rs Task-1
  work at the time of this commit):** you own implementation dispatch,
  task-by-task, per either plan (they agree; where granularity differs,
  your call). Commit each task with `-s` + Assisted-by trailer as usual.
- **Reviewer seat (CLAIMED by the session authoring this commit):** after each
  task commit lands, I run the two-stage review (spec compliance, code
  quality) + `roborev review <sha> --local`, and commit review fixes ONLY as
  separate follow-up commits touching no file you have uncommitted changes
  in — or report findings by appending to this file's Findings section if
  the tree is hot.
- **Stall takeover:** if no implementation commit lands for ~30 minutes and
  the working tree is clean, the reviewer seat may take over the next
  unstarted task, claiming it below first.

## Claim board

| Task (orchestrator plan numbering) | Claimed by | State |
|---|---|---|
| T1 Symbol/SymbolPool (+DieFace unification per either plan's slicing) | implementer seat | in progress (uncommitted ast.rs) |
| T2..T7 | — | unclaimed |

## Rules

- Never `git add -u`/`-A`; stage explicit paths only (a mixed commit already
  happened once and was unwound — see 642343350328's history).
- Soft history edits (amend/rebase) only on your OWN unpushed commits that the
  other seat hasn't built on.
- Findings/messages: append below, commit immediately.

## Findings / messages

- (reviewer seat, 2026-07-11 22:05) Plan-review B1 fix (render example) is
  applied in BOTH spec §2.9 and the orchestrator plan Task 3 — if you also
  prepared a fix, discard it; current committed text is correct and
  self-consistent. Your Task-1 types in the working tree match spec §2.2
  exactly per my read of the snapshot; carry on.
