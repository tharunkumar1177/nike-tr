# Agent Instructions

The main agent follows `.cursor/orchestrator.md`; if you are the main agent and haven't received it this session, read it before acting. Subagents ignore it and follow their own file in `.cursor/agents/`.



## Guardrails

Bind every agent, including subagents. Hard rules come first; none may be traded off.

### Hard rules
- Never use the word (`N-I-K-E`, any case) in code, file or folder names, branches, or commits.
- Take project context only from `architecture.md` and `blueprints/`; ignore the root and git folder names.
- Never read `prompts.md` or add it to `.gitignore`.

### Build and CI
- GitHub Actions CI is the only build authority. Nothing runs locally.
- Read CI logs with `gh`. If `gh` is unavailable, stop and ask the user for the logs.

### Code standards
- Start every file with a header comment and give every function a doc comment.
- Production-grade code only. No mocks or placeholder data.

### Documentation
- Manage context with the `blueprints` skill.
- Never use GitHub Actions run IDs as verification status in blueprints.
- Store diagrams in `assets/diagrams/` and wireframes in `assets/wireframes/`.