# Agent Instructions

This file has two parts. **Guardrails** bind every agent, including subagents. **Orchestrator** applies only to the main agent; subagents skip it and follow their own file in `.cursor/agents/`.

---

## Guardrails

- Build production-grade code only. No mocked UI or placeholder data.
- Never write the brand word (N-I-K-E, any case) in code, files, names, or commits.
- GitHub Actions CI is the only build authority. Nothing runs locally.
- Read CI logs with `gh`. If `gh` is unavailable, stop and ask me for the logs.
- Never read `prompts.md`.
- Use the Blueprints skill to manage context.

---

## Orchestrator (main agent only)

You implement. Subagents research, plan, design, and diagnose so your context stays on the code.

### Team

| Subagent | Use for | Access |
| --- | --- | --- |
| `researcher` | How the reference product or unfamiliar code works | read-only |
| `architect` | Stack, data model, contracts, slice plans | read-only |
| `designer` | UI, UX workflows, and design plans | read-only |
| `reviewer` | Poll CI runs and diagnose root causes of failures | read-only |

Don't call `productmind` directly. `researcher` and `designer` call it when they need user-sentiment input.

### Delegation rules (mandatory)

| When | Delegate to | Don't do it yourself |
| --- | --- | --- |
| You need facts about the reference product, web sources, or code you haven't touched | `researcher` | Web searches, broad codebase searches |
| A slice has no approved plan, or a change crosses module boundaries | `architect` | Choosing stack, schema, or contracts |
| Anything a user sees or interacts with changes | `designer` | Inventing layouts, styles, states, or flows |
| You pushed a commit | `reviewer` | Reading or polling CI logs |

Before each action, ask: is this research, planning, UI design, or CI diagnosis? If yes, delegate it. Never start code for a slice until its architect plan exists and, for UI, its design plan exists.

### Context budget

Your context window is about 300k tokens. Treat it as the scarcest resource.

- **The repository is the memory, not the chat.** Durable knowledge goes into `architecture.md`, `blueprints/`, and `ROADMAP.md`. Re-read those files instead of relying on earlier conversation.
- **Delegate small, bounded tasks.** One delegation covers one question, one slice step, or one screen.
- **Send pointers, not payloads.** In delegation prompts, reference file paths and blueprint headings. Don't paste their contents.
- **Ask for compact returns.** Request each agent's defined output format, capped at about 400 words (designer plans up to about 800). Don't pull full logs or research dumps into your context.
- **Read only what the plan lists.** Open the files named in the architect and design plans, not their neighbors.
- **Checkpoint:** when your context feels heavy, or at the end of each slice, update `ROADMAP.md` so a fresh session can resume from the files alone.

### Session start

1. Run `gh auth status` and confirm access to the repository. If it fails, apply Guardrail 5 and stop.
2. Read `ROADMAP.md` and `blueprints/README.md` if they exist. Resume from the next unchecked slice.
3. If neither exists, run **Phase 0**.

### Phase 0: Foundation (once)

1. `researcher`: survey the reference product's feature set and system behavior, one area per call, not the whole product in one go.
2. `architect`: propose the stack, the system boundaries, and an ordered list of **vertical slices**. Each slice is a thin, user-visible increment that can be shipped and verified in CI alone.
3. Write `architecture.md`, `blueprints/README.md`, and `ROADMAP.md` (the ordered slice checklist) using the `blueprints` skill.
4. **Stop and get user approval** for the stack and the slice plan before building anything.
5. Slice 1 is always the walking skeleton: repo scaffold and GitHub Actions workflow (install, lint, type-check, test, build). The forbidden word is enforced by Guardrail 4, not by a CI check. Dependencies are declared with versions checked against the registry. Lockfiles are produced in CI, never guessed.

### Build loop (one slice at a time)

Repeat for each unchecked slice in `ROADMAP.md`:

1. **Scope:** pick the next slice. Load its manifest route: one primary blueprint and only its linked architecture anchors.
2. **Research (only if needed):** `researcher` answers unknowns specific to this slice.
3. **Plan:** `architect` returns ordered steps, files affected, and acceptance criteria for this slice only. If the plan changes a shared contract in `architecture.md`, get user approval first.
4. **Design (if the slice has UI):** `designer` returns a design plan per screen or flow, one call each.
5. **Build:** implement the plan steps in order, following the design plan exactly. If a plan is wrong or incomplete, send it back to its agent instead of improvising.
6. **Verify in CI:** commit on the slice branch with a clear message, push, and hand the branch and run ID to `reviewer`.
 - Green: continue.
 - Red: fix the root causes the reviewer returned. Retry at most 3 times, then stop and report the blocker to the user.
7. **Record:** update the slice's blueprint status and acceptance checkboxes, update the manifest if routes changed, check off the slice in `ROADMAP.md`, then open a PR and merge it once CI is green.
8. **Report:** give the user a short slice summary: what shipped, the CI run link, and what's next.

### Delegation prompt template

Every delegation must stand alone, because subagents don't see this conversation:

```text
Goal: <one sentence, one responsibility>
Context: read <blueprint path> and <architecture.md#anchor>; relevant code: <paths>
Constraints: follow AGENTS.md Guardrails (CI-only, no mocks, forbidden word, never read prompts.md)
Acceptance: <observable criteria from the slice plan>
Return: your standard output format, compact
```

### Escalate to the user when

- The stack, the slice plan, or a shared contract needs a decision or change.
- An architectural flaw is found. Present the options (including "stick with the existing system") and wait.
- CI still fails after 3 fix attempts, or `gh` access is lost.
- Requirements are ambiguous in a way that changes user-visible behavior.


