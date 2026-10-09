# AI design

## Outcome or responsibility

The user can design with an LLM as an alternative entry point. They can describe a circuit and receive a verified, editable schematic, ask for a review of their design, and get explanations of ERC, DRC, and simulation results. The model writes a code-like circuit DSL, never geometry. Placement and routing remain classical algorithms.

## Current verified status

**Status:** Planned

## Architecture dependencies

- [AI generation with verification](../architecture.md#ai-generation-with-verification)
- [Secrets and network](../architecture.md#secrets-and-network)
- [Design document ownership](../architecture.md#design-document-ownership)
- [Error and recovery](../architecture.md#error-and-recovery)

## Local rules and implications

### AI generation with verification

The DSL is a small declarative language for components, nets, and simulation assertions, compiled by `crates/ai`. A candidate design is offered to the user only after it compiles, passes ERC, and passes its assertions. The user can see the failure log from each retry. The retry limit is configurable and defaults to 3.

### Secrets and network

Settings accept any OpenAI-compatible endpoint, including a local server, along with a key held in the operating system credential store. Before the first request to a new endpoint, the user sees which data will be sent.

### Design document ownership

Merging generated output is one undoable command. Review and explanation features are read-only.

### Error and recovery

Network, rate-limit, authentication, and malformed-output errors each map to a distinct `AppError` kind with an actionable message. Requests can be cancelled.

## Related blueprints

### Required

- [schematic-capture](schematic-capture.md) — the ERC and netlist used for verification, and the merge target.
- [circuit-simulation](circuit-simulation.md) — headless runs for assertions.

### Impact checks

- [component-library](component-library.md) — when part search is exposed to the model as a tool.

## Relevant implementation and tests

- `crates/ai` — Planned

## Acceptance or verification criteria

- [ ] Endpoint and key settings are saved, and the key never appears in project files or logs.
- [ ] Design review cites the specific components and nets it comments on, with jump-to-location.
- [ ] Prompt-to-circuit produces a schematic only after it compiles, passes ERC, and passes its assertions. Failures are visible.
- [ ] The DSL compiler has unit tests covering valid input, syntax errors, and unknown parts.

## Remaining gaps and unknowns

- Whether an AI suggestion for initial board placement is worth adding is undecided, and it is deferred until after slice 11.
