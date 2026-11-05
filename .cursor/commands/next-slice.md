# Next Slice

Resume the build loop from the next unchecked slice.

1. Run `gh auth status`. If it fails, stop and ask me for access.
2. Read `ROADMAP.md` and `blueprints/README.md`. Identify the first unchecked slice.
3. Load only that slice's primary blueprint and its linked `architecture.md` anchors.
4. Tell me which slice you picked, its acceptance criteria, and whether it has an approved architect plan and (for UI) a design plan.
5. If a plan is missing, delegate to `architect` (and `designer` for UI) using the delegation prompt template in `AGENTS.md`.
6. Stop and wait for my go-ahead before writing any code.

Follow every Guardrail in `AGENTS.md`.
