# Rule: Plans are stateful, checkbox-driven, and resumable

Every non-trivial task in this repo runs against a written plan in `.docs/plans/`. Plans are not write-once documents. They are the live source of truth for "what is done, what is next, where do we pick up if the agent stops."

## Plan structure (mandatory)

Every plan file MUST have this frontmatter:

```yaml
---
status: in-progress | done | abandoned
created: YYYY-MM-DD
updated: YYYY-MM-DD
goal: <one sentence>
---
```

And this body structure:

1. **Goal** (one sentence, same as frontmatter)
2. **Inputs** (rules and learnings consulted)
3. **Affected files** (path + one-line change description)
4. **Risks / Unknowns**
5. **Done criteria**
6. **Milestones** - the heart of the plan. Each milestone has:
   - A short title
   - A one-line outcome
   - A checklist of tasks, each as `- [ ] ...` checkboxes
   - Tasks are independently verifiable. No task should be larger than a single focused work session.
7. **Log** - append-only section. Each entry is `- YYYY-MM-DD HH:MM <short note>`. Used to record decisions, deviations, blockers, and resume points.

## Execution protocol

- The implementer ticks `- [ ]` to `- [x]` **as soon as a task is finished**, before moving to the next task. Not at the end of the milestone, not at the end of the session.
- After each tick, the implementer updates the `updated:` field in frontmatter to today's date.
- If the implementer makes a decision that deviates from the plan, it appends a Log entry explaining why and edits the affected milestone or task list to match reality.
- When all tasks across all milestones are checked, the implementer flips `status:` to `done` and appends a final Log entry.

## Resume protocol (this is why the structure exists)

At the start of every session, before doing any new work, the main agent MUST:

1. Glob `.docs/plans/*.md` and read the frontmatter of each.
2. Identify any plan with `status: in-progress`.
3. If one or more in-progress plans exist, surface them to the user with: filename, goal, and the next unchecked task. Ask whether to resume, switch to the new request, or abandon (set `status: abandoned`).
4. Do not silently start new work while a plan is in-progress. The user decides.

If the user starts a new feature request and an in-progress plan is unrelated, that is fine - just confirm explicitly rather than assuming.

## Why
Sessions get interrupted. Context windows fill up. The user closes the terminal. Without a checkbox-driven plan, a partially-finished feature looks identical to a not-started feature, and the agent either redoes work or abandons it. Checkboxes plus a Log give any future session enough information to pick up exactly where the last one stopped.
