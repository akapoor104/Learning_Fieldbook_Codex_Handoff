# Interfaces the agents share

The production app uses Rust/Axum, same-origin REST JSON under `/api/v1`, SQLx and PostgreSQL 16, Svelte/TypeScript, Vite, and Svelte Flow. PostgreSQL owns outline parents, order and typed relations. The graph and List read from the same query. One Rust service also serves the built frontend. A local worksheet, the household backup and a handoff contract each have different version numbers.

## Implemented by the first contract tickets

`core.openapi.json` is an OpenAPI 3.1.1 starter containing the probes, graph read and atomic split. `planning-examples.json` supplies valid UUID payloads and the alias map used by the family-facing scenarios. Run the same example through Rust, SQL integration tests and the Svelte consumer. Use the supplied JSON as a starting point for the repository’s full `api/openapi.yaml`. Health payload literals and the example cookie/header names are implementation conventions for the first ticket. Existing compatible names in a target repository may be reconciled in its OpenAPI and tests.

| Operation | Observable behavior |
| --- | --- |
| Split | A successful write with the expected revision commits all children. 201 with Location and ordered IDs. Same actor, request and key replay the original successful result after authorization, even if the map has advanced. A different body for the same key or stale fresh key yields 409. Blank titles yield 422. |
| Graph | Return at most the focus plus 24 visible neighbors. Authorize first, then compute pages, counts and edges. Exclude any edge with a hidden endpoint. Frontend layout coordinates do not define relationships. |
| Permissions | Derive household and learner from verified access. Check references in both path and body. A signed-in caller sees the same 404 for absent and inaccessible objects. Protect cookie-authenticated changes against CSRF. |
| State transitions | HTML form save redirects with 303, JSON create uses 201 with Location, edit returns 200 with revision and withdrawal generally uses 204. A scoped deletion that returns a recovery receipt documents that response explicitly. |
| Conflict | Keep the submitted draft and return details safe for this caller. Retry the exact accepted command before testing a stale revision. The same key can never duplicate a row. Do not cache an authorization grant inside a receipt. |

## Additional surfaces to document as tickets land

| Tickets | API or screen family | Required behavior |
| --- | --- | --- |
| B1–B5 | WebAuthn, session switch, moments, uploads, diary, grants, comments | One credential per actor. Explicit read scope, resumable chunks, original vs approved preview and revocation on each read. Browser form routes are in spec §6. |
| B6–B9 | Projects/skills, Explore, check-ins, outside-help preview/send, Keepsakes | One evidence source ID, one suggestion, versioned consent, frozen one-page editions. |
| B12–B17 | Maps, nodes, splits, edges, handoffs, checks and observations | Structural, creative and handoff edges have separate semantics. Only a real observed trial may create a journal source. |
| B18–B20 | Reading-order, writing-workspace, graph, moments and portfolio-items | Restricted Docs pointer, no server fetch, dated PDF, optional date precision, explicit selection and withdrawal. |
| B21–B22 | Project Trash/restore, removed-items/restore, edit undo/redo | Current permission, same IDs, previous order, safe conflicts and retained withdrawals. |
| B24/B25 | Pretotype plan and observation, prototype inputs, résumé selection and revival | See [project trials API](trials.openapi.json). Require one or more observed same-project source revisions, retain links in both directions, and project only permitted evidence. |
| B23 | Authorized worksheet projection and manual completion batch | Stable item codes, optional in-scope fields, round-trip marks, no automatic evidence. |

## Suggested wire shape for B23

The design specifies behavior without naming these two paths. These names are proposed so frontend and backend agents can start independently. Record the final names and OpenAPI at the production join.

- `GET /api/v1/maps/{map}/worksheet?scope=branch&root={node}` returns the prepared date, project and branch identifiers, authorized ordered rows with stable paper codes, current completion and item revision. Whole-project scope omits root. Optional notes and handoffs are independently requested. Use project-scoped stable IDs behind printable codes, resolve collisions permanently at creation, and match an earlier printed code after a rename or reorder.
- `PATCH /api/v1/maps/{map}/completion` receives an actor-scoped idempotency key and `{"changes":[{"node_id":"<uuid>","expected_revision":3,"completed":true}]}`. Check every record’s current permission and revision and commit the batch together. Return updated IDs/states and a reversible receipt. A stale item reports a conflict, and a missing or inaccessible item uses the scoped 404 rule. No journal, portfolio or contract-check write occurs.
- Dates, item aliases and sample codes in explanatory fixtures are fixed test inputs. Application IDs at the HTTP boundary are real UUIDs. Compare relationships by identity and actual results, with stable alias mapping for reports.

See `reference/Learning_Fieldbook_Design_Spec_v0.8.md` §§4–6 for the existing contracts and §9 for recovery. Reviewed infrastructure guidance is linked in `guide/MODELS_AND_SOURCES.md`.

Use the operation definitions in `trials.openapi.json` together with the shared authentication, CSRF, idempotency and conflict rules above. Trial aliases in the teaching fixture map to real UUIDs in the product adapter.
