# Rule: Agent docs must stay in sync with agent files

Whenever a file in `.claude/agents/` is added, removed, renamed, or changed in a way that affects its `description`, `tools`, `model`, or core behavior, the following MUST be updated in the same change:

1. The **Available agents** table in `AGENTS.md`.
2. The **Routing heuristics** subsection in `AGENTS.md`.

`AGENTS.md` is the single source of truth; `CLAUDE.md` is only a pointer to it and needs no update.

## Why
The table and routing heuristics are how agents (and humans) decide which subagent to invoke. If the docs lag the actual agent files, callers route to stale behavior, the wrong agent gets used, or a new agent goes unused entirely. Self-improvement breaks down when the index is wrong.

## How to apply
- Any agent that edits `.claude/agents/` (including the learner editing itself) is responsible for updating the docs in the same turn.
- The reviewer treats out-of-sync docs as a **blocker** finding.
- The learner, if it ever sees them out of sync from a past session, fixes the sync as its first action before doing anything else.
- Adding a row to a table is not enough. Verify the row's `When to call` column and the corresponding `Routing heuristics` line both reflect the agent's current `description` field.
