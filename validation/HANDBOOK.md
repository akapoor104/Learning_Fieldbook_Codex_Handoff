# How to validate the Fieldbook

## 1. Check the delivery

Run `python3 tools/fieldbook.py check --sealed` on a freshly unpacked kit. It verifies issued file digests and inventory, the 78-slice dependency graph, scenario coverage and four synthetic original-file hashes. Run `python3 -m unittest discover -s tests -v` for the tool regressions. The release contains working Python tools with standard-library dependencies. Production Rust, Svelte, PostgreSQL and host behavior require their own implementation and testing.

For GitHub projects, an optional packet-only CI example is in `templates/ci-example.yml`. The issued checks and their limits are recorded in `validation/ISSUED_CHECKS.md`.

## 2. Run each implemented slice through its normal entry

From the packet directory, use `python3 tools/product_check.py --repo /path/to/checkout --suite rust` or select `web`, `home-config` or `all`. These commands expect the corresponding production lockfiles and tools. The web suite uses the project’s non-watch test script. A missing suite exits with an error. `--show` prints commands without running them. For B0.1, run the Rust tests on the new router. Once B0.2 exists, `python3 tools/probe_health.py http://127.0.0.1:8080` checks the live status bodies. Stop the disposable database and repeat with `--expect-not-ready`, then restart the database.

| Layer | What should supply the proof |
| --- | --- |
| Domain rule | Input, expected output, deterministic Rust unit test and a counterexample. |
| Persistence and REST | Migrated PostgreSQL 16, actual handler and a fresh session. Check revision, row count, retry, actual status, original identity and another actor’s denied path as relevant. |
| Svelte UI | Rendered component or browser test through visible controls. Record desktop and 360 px, keyboard List, focus, 200% zoom and route reload. |
| Android / shared computer | State device, browser, credential type and actual user journey. Virtual authenticator and responsive emulation get separate labels. |
| Print | Use browser print to PDF and request Letter and A4 with typical margins. Inspect a whole plan, one folded branch, an empty plan and 90+ steps with long labels. Also open downloaded HTML offline. Record physical printer output separately when tested. |
| Hosting | Same candidate release and disposable fictional data. Record configuration validation and live rehearsal separately for home, Render and Google Cloud. Cloud deployment/charges need the family’s separate decision. |

## 3. Run the case driver against a real adapter

```sh
python3 tools/run_cases.py --list
python3 tools/run_cases.py --ticket B12 -- python3 examples/lens_reference.py
```

The 25 scenario vectors cover selected high-risk behaviors. The complete product requirements remain T0–T25, including items whose best evidence is a device test or clean restore. Four lens cases and six trial cases have teaching adapters. Run the additional commands below. The production adapter drives the real Rust service and PostgreSQL.

```sh
python3 tools/run_cases.py --ticket B24 -- python3 examples/trials_reference.py
python3 tools/run_cases.py --ticket B25 -- python3 examples/trials_reference.py
```

Implement a **test-only local adapter** in the application checkout. It receives one JSON object on stdin, `{"case":"…","input":{…}}`, and writes **only the observed JSON projection** on stdout. The runner launches a new adapter process for each case, provides the input and withholds the expected output from stdin. The source file `validation/cases.json` documents the comparison. Use an isolated fixture namespace per run. Map test aliases onto generated UUIDs and report identity equality or authorized visible sets from real application results. Keep debugging logs on stderr and use only fictional data.

For example, with `paper-retry` the adapter prints the observed states after mark, identical replay, reopen, undo and redo and the before/after differences in moment, portfolio and passed-check counts. For role cases, issue the ordinary read/save/download commands as the actual seeded actor. Do not copy expected JSON into the adapter. Use the real renderer when a case claims a UI or print result. Cross-device and real restore evidence supplements the DTO comparison.

```sh
python3 tools/run_cases.py --case paper-retry -- python3 /path/to/checkout/tests/fieldbook_adapter.py
```

The adapter’s path is supplied by the future implementation. Unsupported cases must exit nonzero. Each list or Boolean in the JSON has a meaning; an empty selection, a wrong type, extra output field, timeout or failing command never qualifies as a pass. For a longer clean-instance restore use `--timeout 180` ahead of `--`.

## 4. Evidence for a parent ticket

From the packet directory, generate a pending receipt. Put sanitized console logs or brief manual/device observations alongside it.

```sh
python3 tools/fieldbook.py template B2 > evidence/B2.json
python3 tools/fieldbook.py review B2.1
```

Each receipt contains the three slices, canonical T case and its applicable case IDs. Replace every `not-run` with what actually happened. For a passing row set `state` to `passed`, add `kind` (`command`, `manual` or `device`), the full procedure, observed result, file path relative to the evidence directory and its SHA-256. For a command, include numeric `exit_code: 0`. A summary log may be shared across rows when it records each named check separately. The candidate full Git SHA binds the evidence to a revision. Record implementer and independent reviewer for access/recovery tickets. Run `python3 tools/evidence_hash.py evidence/my-check.log` for the digest. For a failure or blocked check leave its actual state and reason in the receipt. The gate stays open until it is re-run.

Use `templates/HANDBACK.md` for the human report. A fresh assignment can read that report and rerun one failure without loading the full project history. Evidence files can establish that a run was recorded; the reviewer still reads them and checks how the adapter obtains its values.

## 5. Product gates

| Gate | What a green evidence report means |
| --- | --- |
| `notebook` | Integrated B0, B1 and B2 ran on the same candidate. The two-device and other checks in those tickets count where specified. Appropriate for synthetic notebook work. |
| `family-pilot` | Includes the B24/B25 path from pretotype to cumulative build, printable résumé and revival. Scoped family pilot tickets plus their prerequisites ran. The selected host has a live synthetic restore/rehearsal. Review all additionally enabled features before introducing personal records. |
| `full` | All 26 canonical T cases and their slices and applicable vectors have evidence on the same candidate commit. The alternate cloud profiles may be configuration-only as allowed in the design. |

```sh
python3 tools/fieldbook.py gate notebook --evidence evidence
python3 tools/fieldbook.py gate family-pilot --evidence evidence
python3 tools/fieldbook.py gate full --evidence evidence
```

For the last two gates, copy `templates/profiles.pending.json` into the evidence directory as `profiles.json`. Select one host, record the full commit SHA, a status of `live-tested`, `config-only` or `not-run`, and a log digest per host. The selected host requires a live rehearsal on synthetic data. Other profiles require at least a verified configuration. Add `--revision FULL_COMMIT_SHA` when checking a specific candidate. Mixed, stale or missing evidence blocks the gate. The checker neither deploys nor sends anything.

## 6. Family and operator acceptance

- On his usual computer, invite Anush to save one sentence in 90 seconds and later find an earlier example in two minutes. Try an optional five-minute weekly review. Ask him to choose an interest outside the starter examples and report his words and where he needed help.
- On Android, a parent uploads the sample picture or PDF. On the computer, the learner matches it later. Exercise both arrival orders, an interrupted upload, sign-out/browser Back and a revoked guest. Each parent separately demonstrates recovery; Anush finds both access events.
- Ask Anush to start the first idea, insert a step between two others, split a child, reorder and restore a branch, and explain a rehearsal versus a real handoff. Print one branch, rename an item and match its old code on return. Record one new observation only when he chooses to.
- For the actual chosen host, restore the six-moment fixture in a new instance, compare file lengths and SHA-256, open the teacher letter, play the short demo, and confirm the original settled permissions. Re-run with a corrupted blob and a revocation newer than the backup. Record source and rollback results.
- On the optimized build, separate initial journal JavaScript from optional graph assets. Measure the 150 KiB compressed and two-second editor targets on the actual device. At 1,000 moments and 2,000 nodes, record request population, warm/cold sample counts and measured 300 ms p95 warm neighborhood target. Rehearsals and unmeasured targets retain their own labels.
