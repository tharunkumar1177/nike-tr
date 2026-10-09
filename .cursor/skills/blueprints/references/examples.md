# Generic Examples

Use these examples to decide document ownership and loading scope. Replace all illustrative names with the repository's actual concepts.

## Isolated feature change

Prompt:

> Allow a user to mute one notification category.

Route:

1. Load the manifest.
2. Load `user-preferences.md` as the primary blueprint.
3. Load only the linked identity, ownership, and policy architecture sections.
4. Inspect preference storage and policy tests.
5. Load `notifications.md` only if muting changes delivery behavior rather than preference storage alone.

Documentation effect:

- Put the shared meaning and ownership of a preference in `architecture.md`.
- Put mute-category behavior, UI/API consequences, status, and acceptance criteria in `user-preferences.md`.
- Add notification delivery as an impact check if the domains can drift independently.

## Cross-cutting identity change

Prompt:

> Replace numeric entity identifiers with globally unique string identifiers.

Route:

1. Load the complete `architecture.md`.
2. Find every manifest entry linked to the identity contract.
3. Load every listed consumer blueprint.
4. Inspect serialization, persistence, API, migration, and test code.

Documentation effect:

- Change the identifier invariant once in the architecture contract.
- Update each consumer blueprint with feature-specific migration and validation consequences.
- Update manifest code areas or impact checks if ownership changes.

## Persistence contract change

Prompt:

> Make saves atomic and recover safely after interruption.

Route:

1. Treat the request as cross-cutting if multiple features save through the same lifecycle.
2. Load the complete persistence, state-lifecycle, and failure-recovery architecture context.
3. Load every blueprint linked to those headings.
4. Inspect transactional boundaries and recovery tests before claiming support.

Documentation effect:

- Define atomicity, visibility, rollback, and recovery centrally.
- In each feature blueprint, state when the feature enters the shared pipeline and what the user observes on failure.

## Creating the system from scattered documents

Starting state:

- one large design document;
- numbered feature notes;
- implementation notes with stale paths;
- no routing instructions.

Process:

1. Inventory claims and verify them against current code and tests.
2. Extract only shared contracts into `architecture.md`.
3. Group numbered notes by cohesive feature domain, not chronology.
4. Create one blueprint per domain with explicit architecture dependencies.
5. Create the manifest last, after dependency and code-area relationships are known.
6. Add a concise manifest-first protocol to applicable agent instructions.
7. Update inbound links, then remove superseded files only when migration is authorized.

## Reconciling stale prose

Observed conflict:

- a blueprint says a feature is complete;
- code implements only the basic path;
- tests cover no failure behavior.

Correct response:

- mark the status `Partial` or `Unknown`;
- cite the verified basic path;
- move missing failure behavior to acceptance criteria or remaining gaps;
- do not rewrite code merely to make it match stale documentation unless implementation was requested.

## Ownership examples

### Shared invariant

Architecture:

> Events use UTC timestamps and preserve the original source timestamp.

Blueprint implication:

> Notification ordering uses the preserved source timestamp; retries must not generate a new logical timestamp.

Do not restate the entire timestamp schema in the blueprint.

### Feature-specific behavior

Blueprint:

> Muting a category suppresses user-facing delivery but retains the event in history.

Keep this out of architecture unless multiple feature domains must obey the same suppression rule.

## Routing test matrix

| Representative prompt | Smallest sufficient load |
|---|---|
| “Change the color of one notification badge.” | Notification blueprint plus its linked presentation contract, if one exists |
| “Add a second delivery provider.” | Delivery blueprint plus provider and failure contracts; impact-check preferences only if policy behavior changes |
| “Change the shared event schema.” | Complete architecture plus every manifest-listed event-schema consumer |
| “Fix an account-preference migration.” | Preferences blueprint, migration/lifecycle contracts, and persistence impact checks |
| “Update unrelated CI caching.” | No blueprint unless the manifest explicitly routes build infrastructure |

## Anti-patterns

| Avoid | Prefer |
|---|---|
| Routing only by a shared source filename | Route by task concepts and intent |
| Copying a formula or schema into every blueprint | Define it once in architecture and state consequences in blueprints |
| Loading every blueprint for an isolated edit | Load one primary blueprint and linked contracts |
| Marking a feature complete from old prose | Verify current code and tests |
| Numbered filenames such as `feature-03.md` | Stable domain names such as `notifications.md` |
| Duplicating the router in multiple instruction files | Keep one manifest and link to it |
