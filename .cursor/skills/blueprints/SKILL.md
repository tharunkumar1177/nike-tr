---
name: blueprints
description: Create and maintain architecture.md and blueprints/ as a progressively disclosed technical documentation system. Use when establishing or changing shared architecture contracts, independently routable responsibility blueprints, manifest-based task routing, documentation migrations, or evidence-backed alignment between design documentation, code, and tests.
---

# Blueprints

Maintain a documentation system that gives each task the smallest sufficient design context while keeping shared technical contracts authoritative and discoverable.

A blueprint is the focused, independently routable record for a durable responsibility and the significant design context needed to change it safely. A responsibility may be a capability, domain, component, integration, or workflow; it is not created merely because a folder, service, class, or ticket exists.

## Evidence rules

- Read applicable repository instructions before editing and preserve unrelated changes.
- Treat inspected code and tests as authoritative for current behavior.
- Treat explicit requirements as authoritative for intended behavior.
- Label conflicts, uncertainty, and unverified claims instead of guessing.
- Do not mark implementation complete from documentation alone.

## Fast routing

Start with `blueprints/README.md` when it exists.

| Task type | Load | Then |
|---|---|---|
| Local responsibility change | Manifest, one primary blueprint, and its linked architecture anchors | Inspect listed code and tests; load impact checks only when their trigger applies |
| Shared-contract change | Manifest, complete `architecture.md`, and every manifest entry linked to affected anchors | Inspect and update each consumer |
| New or uncertain scope | Manifest and relevant code and tests | Classify ownership before creating or expanding documents |
| Bootstrap, normalization, or migration | Existing design documents, relevant implementation evidence, and affected instructions | Read the templates; read examples only for ambiguous ownership or routing decisions |

Do not load unrelated blueprints. When routing is uncertain, inspect code and tests before expanding documentation context.

## Decide document ownership

### Shared architecture contracts

Put a rule in `architecture.md` when two or more blueprint responsibilities must obey it, or changing it can affect two or more responsibilities. Shared material commonly includes:

- system boundaries and dependency direction;
- identity, ownership, shared representations, and data lifecycle;
- persistence, concurrency, recovery, security, and error semantics;
- cross-cutting flows and verification boundaries.

Give every reusable contract a stable, descriptive heading. Define each invariant once, including units, coordinate spaces, ordering, ownership, and failure semantics where applicable.

### Blueprint responsibilities

Put material in a blueprint when it describes a responsibility-local outcome, rule, acceptance condition, implementation boundary, migration concern, or concrete consequence of a linked shared contract.

Create a blueprint only when all responsibility tests pass:

1. **Coherence** — it has one meaningful responsibility or ownership boundary.
2. **Independent change surface** — it has distinct rules, lifecycle, evidence, implementation or test areas, dependencies, or operational risks.
3. **Routing value** — a future task can load it without unrelated documentation.

When the purpose is to preserve a design decision, also require that the decision is hard to reverse, non-obvious without context, and involves a genuine trade-off. Record shared decisions in `architecture.md`; record responsibility-local decisions in the relevant blueprint.

Do not create a blueprint for a routine, local, reversible implementation detail; a folder, class, or ticket with no independent responsibility; or a topic whose evidence is too weak to define responsibly.

### Authority boundaries

The manifest is the sole detailed routing and consumer index. `architecture.md` owns shared contracts; blueprints own local implications. Do not duplicate routing tables, make two documents authoritative for the same invariant, or restate a shared contract in a blueprint.

## Documentation model

### `architecture.md`

- Define shared contracts and cross-cutting flows under stable headings.
- Link to the blueprint manifest instead of embedding complete responsibility specifications.
- Distinguish enforced contracts, intended requirements, and unverified claims.

Read [references/templates.md](references/templates.md) only when creating `architecture.md` or normalizing an inconsistent structure.

### `blueprints/README.md`

Use the manifest to route tasks by intent, not only by filenames. For every responsibility, record:

- concepts and synonyms;
- responsibility type;
- primary blueprint;
- direct Markdown links to exact applicable `architecture.md` headings;
- required blueprint dependencies;
- impact-check blueprints with the condition that triggers each check;
- principal implementation and test areas.

Use this table shape:

| Concepts and synonyms | Responsibility type | Primary blueprint | Required architecture contracts | Required blueprints | Impact checks | Principal implementation and tests |
|---|---|---|---|---|---|---|

Use `None` explicitly when no required or impact-check dependency is known. Prefer responsibility-domain filenames in kebab case; avoid chronological names such as `feature-01.md`.

Define this loading protocol in the manifest:

1. Match task intent and synonyms.
2. Load one primary blueprint and only its required architecture anchors.
3. Inspect listed implementation and tests.
4. Load an impact-check blueprint only if its stated trigger applies.
5. For a shared-contract or ambiguous change, load the complete architecture and every manifest entry linking the affected anchor.
6. Do not load unrelated blueprints.

When repository instruction files need progressive-loading guidance, add only a concise manifest-first protocol and link to the manifest. Do not duplicate its routing table.

### Responsibility blueprints

Keep every blueprint independently useful after its declared dependencies are loaded. Retain these headings even when the content is `None` or an explicit explanation:

1. `Outcome or responsibility`
2. `Current verified status`
3. `Architecture dependencies`
4. `Local rules and implications`
5. `Related blueprints`
   - `Required`
   - `Impact checks`
6. `Relevant implementation and tests`
7. `Acceptance or verification criteria`
8. `Remaining gaps and unknowns`

Link architecture dependencies to exact headings. For every dependency, explain its concrete consequence without redefining the shared contract. Cite inspected implementation or tests for current-behavior claims, or label the claim unverified.

Use these status values consistently:

- `Implemented` — current behavior is verified by identified code or tests.
- `Partial` — some criteria are verified and the gaps are named.
- `Planned` — intended behavior is not yet verified in code.
- `Unknown` — evidence is insufficient or conflicting.
- `Deprecated` — retained only for compatibility or migration.
- `Superseded` — replaced by `[name](relative-path.md)`.

Read [references/templates.md](references/templates.md) when creating a blueprint or normalizing its structure. Read [references/examples.md](references/examples.md) only for ambiguous classification, routing, migration, or ownership decisions. Do not load either reference for a routine update with a valid manifest route and blueprint.

## Change procedures

### Local responsibility change

1. Update the primary blueprint.
2. Check only its linked architecture contracts and triggered impact checks.
3. Update the manifest if concepts, ownership, dependencies, or code and test areas changed.

### Shared-contract change

1. Update `architecture.md` first.
2. Find every manifest entry linking the changed heading.
3. Review and update each consumer's implications, status, criteria, and gaps.
4. Check relevant implementation and tests for contract drift.

### New responsibility

1. Apply the responsibility tests before creating a document.
2. Decide whether the material is a new responsibility, part of an existing one, or a shared contract.
3. Add or extend a blueprint and update the manifest route.
4. Add required dependencies, conditional impact checks, and verified code and test areas.

### Bootstrap or normalize

1. Inventory existing architecture, specifications, implementation notes, instructions, code, and tests.
2. Extract shared contracts into `architecture.md` and group local material by durable responsibility rather than chronology.
3. Create or normalize blueprints, then build the manifest from verified relationships.
4. Update inbound links and routing instructions.
5. Rename, retire, or delete superseded documentation only when the request authorizes the migration.

## Validate before finishing

### Static topology

- Local Markdown paths and anchors resolve.
- Every non-manifest blueprint appears exactly once in the manifest, and every manifest blueprint exists.
- Manifest contract links point directly to `architecture.md` anchors.
- Every blueprint retains the required headings.
- Renamed, retired, or superseded documents and anchors have no stale inbound links.

### Evidence and semantics

- Claims changed by the task agree with inspected code and tests or are labeled `Planned` or `Unknown`.
- Each shared invariant has one authoritative definition.
- Blueprint implications do not redefine their linked contracts.
- Every manifest-linked consumer of a changed architecture anchor was reviewed.

### Routing behavior

When the change affects routing or structure, test one representative local task, one shared-contract task, and one unrelated task. Confirm that each loads the smallest sufficient document set and that unrelated work does not load a blueprint unless the manifest routes it.

Run repository-provided documentation checks when available. Otherwise use non-destructive link, heading, reference, and duplicate-content checks appropriate to the environment. Report what changed, what was validated, and which claims remain unverified.

## Guardrails

- Do not invent project-specific architecture, status, paths, ownership, or test coverage.
- Do not rename or delete existing documentation without migration authority and updated references.
- Do not turn `architecture.md` into a collection of responsibility specifications.
- Do not make blueprints depend on hidden conversational context.
- Do not duplicate the manifest in repository instruction files.
