# Learning Fieldbook agent instructions

Build the family journal, project planner and portfolio described in design v0.8. Work one assigned slice at a time. Run the real entry point that the family or operator would use.

## Start an assignment

- Read the requested `tickets/Bn.md` card. Use `python3 tools/fieldbook.py brief Bn.m` to get the slice prompt. The source of truth is `reference/Learning_Fieldbook_Design_Spec_v0.8.md`. Use the contract examples for interface work and the relevant walkthrough page for interactions. The earlier workflow review offers pilot suggestions where the spec leaves a choice.
- Inspect the target repo and run its baseline checks first. Reuse existing names, public types, build tools and repo instructions. Record the starting revision and any pre-existing failures. Treat the hosted browser prototype as a UX reference. Use a dedicated production checkout for SQL, secrets and household data.
- Build the smallest complete path. A change to a saved record needs its paired action, stable identity and one recovery test. Check the current actor and item on the Rust server. Use fictional data, temporary test credentials and a disposable database.
- Give related records one shared ID. Keep parentage and order, creative inspiration, data handoffs and evidence links distinct. Model domains, roles in an event, tags and household labels as data. A novel activity must work without adding a domain-specific table or screen.
- Keep child-authored words, adult observation, suggestions and observed outcomes separate. Keep the first capture usable with a single sentence from a shared computer. The child has no phone requirement. Outside AI stays off for development and tests use a recording fake.
- A bounded UI or rule change usually fits a smaller model. Where a task crosses schema, identity, access, migrations, data recovery or concurrency, isolate those changes and use an independent reviewer. Put a short human explanation beside the check Anush can run.
- List exact changed files, executed commands, exit codes, observed behavior, fixture IDs, unresolved failures and anything untested. A passing packet self-test is recorded as packet evidence. Product acceptance needs the actual production checkout and observations. A live cloud deployment, external send or move of real family data is outside a normal coding assignment.

## Suggested paths in a new production repo

| Area | Suggested location |
| --- | --- |
| Rust executable, HTTP routes, policy and rules | `crates/fieldbook/src/` with `domain/`, `http/`, `storage/` and focused integration tests in `crates/fieldbook/tests/` |
| Database | `migrations/` plus disposable PostgreSQL 16 in Compose |
| Web | `web/src/`, `web/tests/` and `web/package-lock.json` |
| Shared public contract | `api/openapi.yaml` and committed DTO examples |
| Operations | `Dockerfile`, `compose.yaml`, `render.yaml`, `deploy/home/`, `deploy/google/` and `fieldbook backup/restore/smoke` |
| Repeatability | Commit `Cargo.lock`, a pinned Rust toolchain, a Node version and the package lock. Keep builds reproducible. |

The tables describe suggested file locations for a new checkout. Preserve a compatible existing layout. Instructions and recorded evidence belong with the code that was actually validated.
