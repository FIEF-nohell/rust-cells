# Project Instructions for AI Agents

> Bootstrapped by nohell v7

This file (`AGENTS.md`) is the routing index for any AI agent working in this repo, and the single source of truth for project instructions. Read it first, every session, before doing anything else. `CLAUDE.md` is a thin pointer to this file so Claude Code loads it; all real content lives here.

## Session start protocol

All relevant docs live in `.docs/`. At the start of every fresh session, before doing any work, you bring yourself up to speed on your own:

1. Read this file (`AGENTS.md`) in full.
2. Read every file in `.docs/rules/` (hard rules, non-negotiable).
3. Read the three most recent files in `.docs/learnings/` (lessons from past mistakes, do not repeat them).
4. Run the resume check: glob `.docs/plans/*.md` and look for any plan with `status: in-progress` (see Resume protocol below).
5. Gather any other context you need (recent git log, the project tree, the project-specific section of this file, `.docs/PROJECT.md` for the locked architecture, `.docs/PROGRESS.md` for current state) to reach a point where you can start working.

The user will usually open the session with nothing more than a greeting like "Hi". When that happens:

- Immediately reply with exactly `Session started`.
- Then silently perform steps 1-5 above.
- When you are done and ready, reply with exactly `Ready to work.` (and, if a `status: in-progress` plan was found, the one-line resume summary the Resume protocol requires).

Do not wait for the user to spell out the rules each session. The handshake is the rule: greeting in, `Session started`, do the reading, `Ready to work.` out.

## Read first, every task, no exceptions

Beyond the session-start protocol, before starting any individual task, re-read as needed:

1. This file in full.
2. Every file in `.docs/rules/` (these are hard rules, non-negotiable).
3. The three most recent files in `.docs/learnings/` (these are lessons from past mistakes, do not repeat them).

If a task touches an area that has a relevant older learning (e.g. you are about to edit reaction code, and there is an old learning tagged `reactions`), read that one too. Use Grep on `.docs/learnings/` to find tag matches.

## Resume protocol (check before starting any new work)

Sessions get interrupted. Before starting a new task, scan for unfinished work:

1. Glob `.docs/plans/*.md` and check the `status:` frontmatter field of each.
2. If any plan has `status: in-progress`, surface it to the user with: filename, goal, the next unchecked `- [ ]` task, and the most recent Log entry.
3. Ask the user: resume the in-progress plan, switch to the new request (leaving the old plan in-progress), or abandon it (set `status: abandoned` with a Log entry explaining why).
4. Do not silently start fresh work while a plan is in-progress.

If the user's request is itself the continuation of an existing plan, jump straight to the implementer with that plan path.

See `.docs/rules/plan-execution.md` for the full plan format and execution protocol.

## Repository layout for AI machinery

```
.claude/
├── settings.json        permissions, hooks, agent registration
├── agents/              subagent definitions (YAML frontmatter)
└── commands/            slash commands

.docs/
├── PROJECT.md           locked architecture + operating doctrine (the contract)
├── PROGRESS.md          living per-milestone log: decisions, roster, tests, perf
├── plans/               implementation plans, one per task
├── learnings/           append-only lessons from past sessions
├── rules/               hard rules, more granular than this file
└── research/            researcher agent's findings
```

Anything markdown that is not user-facing documentation goes in `.docs/`. User-facing docs (`README.md`, `LICENSE`) stay at the root.

## Available agents

| Agent | When to call | Output |
|-------|--------------|--------|
| `planner` | Before any non-trivial change | `.docs/plans/YYYY-MM-DD-<slug>.md` |
| `implementer` | After a plan exists | Code changes, completion note on the plan |
| `reviewer` | After implementer finishes | Structured review with severity findings |
| `researcher` | When you need codebase or external context | `.docs/research/YYYY-MM-DD-<slug>.md` |
| `debugger` | When something is broken and root cause is unclear | Root cause analysis + proposed fix |
| `learner` | After a meaningful task, OR via `/learn` | New entries in `.docs/learnings/`, edits to agents or AGENTS.md |
| `sim-core-dev` | Implementing/changing simulation logic in `pwdr-core` (movement, chunks, temperature, reactions, serialize, tick loop) | Test-first core code, PROGRESS.md update |
| `element-author` | Adding a new material/element or changing element data (density, reactions, transitions, blurb) | `MATERIALS`/`REACTIONS` rows + blurb + defining-behavior test |
| `determinism-guardian` | After any `pwdr-core` change, to verify determinism / zero-graphics-deps / parallel==serial / serialize / Cell size | Pass/fail audit with command evidence |
| `perf-auditor` | After a perf-relevant core change, or to investigate a slowdown | Criterion bench results vs baselines + named bottleneck |

### Routing heuristics

- "Build me X" / "let's add feature X" of any non-trivial size: `planner` -> `implementer` (or `sim-core-dev` for core logic) -> `reviewer` -> `learner`. The planner writes a milestone+checkbox plan to `.docs/plans/`; the implementer ticks boxes live as it goes.
- "Continue / resume / pick up where we left off": find the `status: in-progress` plan in `.docs/plans/`, hand it to `implementer` (or `sim-core-dev`).
- "Change the simulation / movement / chunks / temperature / reactions / the tick loop in pwdr-core": `sim-core-dev` (test-first, enforces the locked invariants). For the macroquad app/UI in `pwdr-app`, that is ordinary `implementer` work.
- "Add an element / new material / make X float / freeze / explode" or tweaking an element's data: `element-author`.
- "Is this still deterministic / did I break the goldens / does pwdr-core still have zero graphics deps / does parallel still match serial": `determinism-guardian` (run it before calling any core change done).
- "Is this fast enough / did this regress perf / why is it slow": `perf-auditor` (it benches and names the bottleneck before any optimization).
- "Fix this bug": `debugger` -> `implementer`/`sim-core-dev` (to apply the fix) -> `learner`.
- "Where is X / how does Y work": `researcher`.
- "I just corrected you / that detour was painful / we discovered a constraint": invoke `learner` immediately, or run `/learn`.

If the user says any of "learn from that", "remember this", "don't make that mistake again", "save this lesson" - invoke the `learner` immediately. The slash command `/learn` does the same thing.

## Self-improvement loop (this is core, do not skip it)

After completing any non-trivial task, invoke the `learner` subagent. Non-trivial means at least one of:
- Involved a bug fix
- Made an architecture or design decision
- Surfaced a constraint that was not previously documented
- Cost time on a wrong turn
- Was corrected by the user

The learner has permission to edit `.claude/agents/**`, `.docs/**`, and `AGENTS.md` without asking. Let it.

If you finish a task and decide it does not warrant invoking the learner, that is fine, but the default is to invoke it.

## Hard conventions

- Plans live in `.docs/plans/`. Filename format: `YYYY-MM-DD-<short-slug>.md`. Format and execution protocol defined in `.docs/rules/plan-execution.md`. Plans carry `status:` frontmatter (`in-progress` / `done` / `abandoned`), milestone+checkbox bodies, and an append-only Log.
- Learnings live in `.docs/learnings/`. Filename format: `YYYY-MM-DD-<short-slug>.md`. Frontmatter required (`date`, `tags`, `severity`, `applies-to`).
- Rules live in `.docs/rules/`. One concept per file. Short, imperative.
- Research notes live in `.docs/research/`. Filename format: `YYYY-MM-DD-<short-slug>.md`.
- Never modify `.docs/rules/` casually. Rules are promoted from learnings or added by the user.
- Never delete from `.docs/learnings/`. The learner can supersede an old learning by writing a newer one and editing the old one to add a `superseded-by:` line in frontmatter.
- `.docs/PROJECT.md` holds locked decisions (do not change them) and `.docs/PROGRESS.md` is the living log - keep PROGRESS.md current with decisions, test status, and perf numbers per change.

### Agent docs must stay in sync (non-negotiable)

If you add, remove, rename, or change the behavior of any file in `.claude/agents/`, you MUST update the same commit/turn:

1. The **Available agents** table above (add/remove/edit the row).
2. The **Routing heuristics** subsection above (add/remove/edit the line that mentions the agent).

This file (`AGENTS.md`) is the single source of truth; `CLAUDE.md` is only a pointer and needs no update. A change to an agent file without a corresponding doc update is an incomplete change. Reviewer agent: flag this as a **blocker** finding if you ever see it. Learner agent: if you find them out of sync from a past session, fix it as your first action.

This rule applies to any agent that edits `.claude/agents/` (including the learner editing itself). See `.docs/rules/agent-docs-sync.md`.

### Commit and PR hygiene (non-negotiable)

- **Never co-author commits as an AI model.** Do not add `Co-Authored-By: Claude`, `Co-Authored-By: AI`, `Co-Authored-By: GPT`, or any similar trailer to commit messages. Do not add equivalent attributions in PR descriptions or release notes. The user is the sole author. This default is permanent unless the user explicitly says "credit Claude as co-author on this commit" or similar for a specific instance.
- **Never include "Generated with Claude Code" or equivalent footers** in commits, PR bodies, issue comments, or any other written artifact unless the user explicitly asks for it.
- **No emojis in commit messages.** Stick to plain text.
- **No em dashes in commit messages, PR bodies, or any prose this project produces.** Use periods, commas, parentheses, or colons instead.

### Image generation

For any image generation or editing task, use the `cc-nano-banana` skill. Default output location for this project's generated images is `assets/` (the project keeps its icon at `assets/icon.svg`; there is no dedicated `assets/images/` folder yet). Source originals are saved per the user's global config.

## Project-specific section

### Stack
- **Rust** (edition 2021) Cargo workspace, resolver 2. Two crates.
- **`pwdr-core`** - the simulation library. **Zero graphics dependencies** (locked). Deps: `rayon` (parallel heat diffusion). Dev-deps: `proptest`, `criterion`.
- **`pwdr-app`** - the desktop frontend. Binary name `rust-cells`. Deps: `pwdr-core` (path), `macroquad 0.4` (window/input/GPU blit), `rfd 0.15` (native file dialogs). Windows build-dep `winresource` (icon).
- Release/bench profile: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`.
- It is a native falling-sand / powder simulation: data-driven materials on a flat grid, deterministic seeded RNG, chunked dirty-rect activity tracking, a temperature field with phase transitions, and a data-driven reaction web.

### How to run
- Run the app: `cargo run -p pwdr-app --release` (window opens; left-drag paints, right-drag erases, `F1` help overlay).
- Test the core (headless, the real test suite): `cargo test -p pwdr-core`
- Lint (CI gate): `cargo clippy -p pwdr-core -- -D warnings`
- Format check (CI gate): `cargo fmt --all -- --check` (use `cargo fmt --all` to fix)
- Benchmarks: `cargo bench -p pwdr-core` (criterion; baselines in README.md)
- Verify the zero-graphics-deps boundary: `cargo tree -p pwdr-core` (must show no windowing/graphics crate)

### Key paths
- `pwdr-core/src/grid.rs` - the engine: `Cell`, `Grid`, movement by phase, chunks/dirty-rects, temperature diffusion, reactions, `step` + `step_parallel`, `serialize`/`deserialize`, `render_rgba`, paint/`paint_line`/`flood_fill` helpers.
- `pwdr-core/src/material.rs` - data-driven `MATERIALS` table, `REACTIONS` table, `Phase`, `blurb`, `user_paintable`. **Adding an element = adding data here.**
- `pwdr-core/src/rng.rs` - seeded xoshiro256** (SplitMix64 seeded). The only randomness source; determinism contract.
- `pwdr-core/tests/proptest_invariants.rs` - property tests; further unit tests live in-module in `grid.rs`/`material.rs`.
- `pwdr-core/benches/sim_bench.rs` - criterion benches. `pwdr-core/examples/showcase.rs` - headless showcase.
- `pwdr-app/src/main.rs` - the macroquad frontend (window, input, palette UI, save/load, undo/redo). `pwdr-app/build.rs` - Windows icon embedding.
- `.docs/PROJECT.md` (locked doctrine) and `.docs/PROGRESS.md` (living log) - read both before touching the sim.
