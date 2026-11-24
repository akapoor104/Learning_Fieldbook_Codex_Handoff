# Delivery order and milestones

**Two independent starts.** B0.1 establishes the Rust service. B12.1 is a pure local rule that Anush can build before accounts or a database exist. Run `python3 tools/fieldbook.py ready --done B0.1 B12.1` to see the next eligible slices. Supply the complete list of completed slice IDs when checking a later milestone. Dependencies in `tickets/index.json` are machine-readable. A ticket’s numeric order identifies it in the design and does not impose an artificial sequence.

| Milestone | Slices to begin with | Family-visible or operational demonstration |
| --- | --- | --- |
| Start locally | B0.1 then B0.2 and B0.3 | A health route, durable fixture across restart, one image and three named deployment profiles. |
| First notebook | B1–B2 | A learner can sign in on a shared computer, save one sentence, sign out and recover no prior user data when switching accounts. Journal entry IDs survive a retry. |
| Two contributors | B3–B6 | A parent on Android and the learner on a computer contribute to the same family moment on different days. Test a teacher PDF, guest withdrawal and both parents’ exceptional access. |
| Encourage and explore | B7 and B12, then B13–B14 | One local suggestion and one small test. Student can keep, reject, combine, park or record what actually happened. B8 is the optional, disabled-by-default service boundary. |
| Build an independently testable plan | B15–B17, then B19 | Order a generic project, use one named input/output sample, integrate the real producer, then view the same neighborhood in Graph and List. B19.1 can start before the integration-check screen. |
| Test and build | B24 after project creation, then B25 | Start a fresh pretotype, accept several observed sources per build and several builds per source. Print a chosen setback and revive it with a new plan. |
| Write, recognize and print | B9, B18, B20 and B23 as dependencies allow | An essay opens in a user-selected Google Doc. A portfolio selection links to an actual moment. A project or branch prints to paper and marks can be re-entered by code. |
| Recover and operate | B10–B11 and B21–B22 | Restore an older version to a clean instance, compare original file hashes, preserve guest withdrawal, rehearse cutover and rollback. Repeat affected archive checks whenever a new record type lands. |

## Bound the first change

B0.1 creates or reuses the root Cargo workspace, a pinned Rust toolchain, the binary’s `serve` entry, one router and a loopback test. Use the status contract in `contracts/core.openapi.json`. Leave database and cloud provisioning to B0.2 and B0.3. B0.2 adds PostgreSQL 16 and `/readyz`, including a 503 when the schema is unavailable. B0.3 supplies host configuration from the beginning. A family journal or OAuth integration is unnecessary for those assignments.

The v0.8 [working example](https://fieldbook-idea-studio.plainalgae4.chatgpt.site) demonstrates project pretotypes, shared evidence, working versions, Failure Résumés and revival, plus the first idea, branching, split, insert, reorder, undo/redo, recovery, handoff details, and paper entry with browser-local storage. Design v0.8 covers durable household identity, uploads, authorization, private diary, portfolio, portable backup, three host profiles and the larger journal as production work. The packet adds delivery tooling and tests. The prototype remains a reference under its existing owner access.

## Keep changes small across agents

The agent handling a migration owns its revision and writes an old-data example. Another agent can build the consumer from the approved DTO and golden fixture. Integrate them through the actual HTTP and persistence paths before declaring the ticket complete. Use one current branch/worktree per change if an existing team workflow supports it. Apply migrations in a single reviewed sequence. When merging changes, rerun shared authentication, REST shapes, affected inverses and migrations on the resulting commit.

Use the handback in `templates/HANDBACK.md`. A failure goes to the agent owning the smallest violated contract. Include the failing input, expected and actual result, revision and command. Escalate a real cross-cutting ambiguity to the integrator with the relevant design passage and two implementable choices. Record a resolved decision once and reuse it.

## Workflow refinements for the family pilot

The included v0.6 workflow review documented earlier mock behavior. Reproduce issues on the current target first. Preserve tab-local drafts (B2/B13), link an existing source by ID (B6/B20), retain an explicitly chosen next step (B7/B14), and test three different creative goals (B12). The review also proposes a unified Recently removed view and one next step across interests. Try those as pilot interface choices alongside required scoped project Trash and Removed items. Record the family’s preference before making either a new product requirement.

For scope by gate and how to run a verified build against a fresh database, see `validation/HANDBOOK.md`.
