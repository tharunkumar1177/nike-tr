# Reusable Templates

Adapt these shapes to the repository. Preserve established terminology and omit sections that have no meaningful content.

## `architecture.md`

```markdown
    # System Architecture

    ## Purpose and authority
    State what this document governs and what sources override stale prose.

    ## System boundaries
    Describe the system, external actors, and excluded responsibilities.

    ## Component model
    Describe stable components, ownership, and dependency direction.


    ## Shared contracts

    ### Identity and ownership
    Define identity, ownership, and lifetime rules.

    ### State lifecycle
    Define valid states, transitions, and invalidation behavior.

    ### Data representation
    Define shared units, schemas, coordinate spaces, or invariants once.

    ### Error and recovery
    Define failure visibility, retry, rollback, and recovery behavior.


    ## Cross-cutting flows
    Describe workflows that span multiple feature domains.

    ## Verification boundaries
    Describe which test levels establish confidence in shared contracts.

    ## Feature blueprints
    See [the feature-blueprint manifest](feature-blueprints/README.md).
```

Use headings that reflect the actual system. Stable headings are important because blueprints link to them as contracts.

## `blueprints/README.md`

```markdown
    # Blueprint Manifest

    ## Loading protocol

    1. Match the task by intent and synonyms.
    2. Load the primary blueprint.
    3. Load only its linked architecture sections.
    4. Load impact checks only when the change can affect them.
    5. Expand to the complete architecture for cross-cutting or ambiguous work.

    ## Router

    | Task concepts | Primary blueprint | Architecture sections | Impact checks | Principal code and tests |
    |---|---|---|---|---|
    | alert, notification, delivery | [notifications](notifications.md) | [Event model](../architecture.md#event-model), [Delivery guarantees](../architecture.md#delivery-guarantees) | [user preferences](user-preferences.md) when filtering changes | notification service; delivery tests |
    | preference, mute, opt out | [user preferences](user-preferences.md) | [Identity and ownership](../architecture.md#identity-and-ownership) | [notifications](notifications.md) when delivery changes | preference store; policy tests |

    ## Status vocabulary

    - `Implemented`: verified in current code and tests.
    - `Partial`: some acceptance criteria are verified.
    - `Planned`: required behavior is not implemented.
    - `Deprecated`: retained only for migration or compatibility.
    - `Unknown`: evidence is insufficient; inspect before changing.
```

Replace the illustrative domains and paths with observed repository concepts.

## BLUEPRINT

```markdown
    # <Feature name>

    ## Outcome
    Describe the user-visible or system-visible result.

    ## Current verified status
    **Status:** <Implemented | Partial | Planned | Deprecated | Unknown>

    List concise evidence and the verification date when useful.

    ## Architecture dependencies
    - [Contract heading](../architecture.md#contract-heading)

    ## Feature-specific implications

    ### Contract heading
    Explain exactly what the shared contract requires this feature to do. Do not redefine the contract.



    ## Related blueprints

    ### Required
    - [Dependency](dependency.md) — explain why this feature cannot be understood or changed safely without it.

    ### Impact checks
    - [Affected feature](affected-feature.md) — explain which changes require checking it.



    ## Relevant implementation and tests
    - `path/to/implementation` — responsibility
    - `path/to/test` — verified behavior

    ## Acceptance criteria
    - [ ] Observable, testable behavior

    ## Remaining gaps
    - Missing behavior, risk, migration, or unverified assumption
```

Use `None` explicitly under a related-blueprint subsection when there are no known entries. This makes the dependency review visible rather than accidental.

## Agent-instruction routing excerpt

```markdown
For feature or architecture work:

1. Read `blueprints/README.md`.
2. Select the task's primary blueprint by intent.
3. Read only the architecture headings linked by that blueprint.
4. Inspect relevant implementation and tests.
5. Load impact-check blueprints only when the requested change can affect them.

For structural or cross-cutting contract changes, read the complete `architecture.md` and inspect every manifest-listed consumer. Do not load unrelated blueprints.
```

Keep this excerpt short. The manifest remains the only detailed routing table.
