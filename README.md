# Learning Fieldbook | Codex implementation handoff

**Product design v0.8 · Handoff v0.8.1 · November 22, 2026.** This packet is for the person building the family app and the agent reviewing each change. It includes the complete pinned design, a visual walkthrough, tasks and executable validation tools. Use this page to get started or open the [two-page quick start](Learning_Fieldbook_Codex_Quickstart_v0.8.1.pdf).

## First ten minutes

1. Extract the ZIP. Run the following commands **from the directory containing this README**. Python 3.9+ is sufficient for all packet checks. On Windows, `py -3` can replace `python3`.
2. Use an empty Git checkout for a new production app, or the existing production checkout if one has been provided. In a new repository, place the packet contents at its root and start Codex there. In an existing repository, leave its files and instructions in place, keep the packet under `handoff/` and point Codex at `handoff/AGENTS.md`. The output of `brief` uses paths relative to the packet. Run product builds at the actual application root.
3. Give Codex the output of the first brief. After it returns, run the review brief on the **same candidate commit**. Assign the next eligible slice with the printed dependency list. Keep the first pass narrow enough to review before adding the database.

```sh
python3 tools/fieldbook.py check --sealed
python3 -m unittest discover -s tests -v
python3 tools/fieldbook.py ready
python3 tools/fieldbook.py brief B0.1
```

| Starting point | Give the agent | What success looks like |
| --- | --- | --- |
| Build the family application | `python3 tools/fieldbook.py brief B0.1` | The Rust binary serves a tested status-only response. B0.2 then adds persistent storage and readiness. |
| Give Anush a 45-minute coding session | `python3 tools/fieldbook.py brief B12.1` and `practice/ANUSH.md` | His Rust function chooses a borrowed method or deliberately returns no suggestion. He can explain one case. |
| Implement cumulative project trials | `python3 tools/fieldbook.py brief B24.1` once its prerequisites pass | Every new project begins with a small test. Later slices link several observations to several working versions. B25 adds the project Failure Résumé and revival. |
| Review a completed change | `python3 tools/fieldbook.py review B0.1` | The reviewer runs a normal path and a failure path, inspects the diff and identifies any missing proof. |

## First prompt to paste into Codex

> Open the Learning Fieldbook handoff README and AGENTS.md. Work on B0.1 only. Run the packet integrity check and print the B0.1 brief. Inspect the actual production checkout, implement the Rust health endpoint and run a real HTTP success check and an unknown-route check. Report changed files, exact commands, exit codes, the observed responses and anything still untested. Leave B0.2 for the next assignment.

For a child-led session, replace B0.1 with B12.1 and use the worked prompt in `practice/ANUSH.md`.

## Find what you need

| File | What to use it for |
| --- | --- |
| [Delivery order](guide/ROADMAP.md) | Choose a branch of work, understand the current mock, pick prerequisites and run milestone reviews. |
| [Agent working agreement](AGENTS.md) | Scope, source precedence, suggested layout, handback and review. |
| [Ticket and scenario index](validation/COVERAGE.md) | Open any of the 26 ticket cards. Each card contains three independently briefed slices and its original T case. |
| [Interfaces](contracts/README.md) and [starter OpenAPI](contracts/core.openapi.json) | Share types, sample operations, endpoint behavior and error cases. |
| [Family access and data](guide/ACCESS_AND_RECOVERY.md) | Role matrix, current permissions after restore, inverse actions, Google boundaries and durable backup. |
| [Validation handbook](validation/HANDBOOK.md) | Run checks, implement an adapter, create evidence, and exercise device, performance and release gates. |
| [Model assignments](guide/MODELS_AND_SOURCES.md) | Use economical models for scoped work and escalate specific failures. |
| [Product spec](reference/Learning_Fieldbook_Design_Spec_v0.8.md), [walkthrough](reference/Learning_Fieldbook_Visual_Walkthrough_v0.8.pdf) and [sample worksheet](reference/Learning_Fieldbook_Sample_Worksheet_v0.7.pdf) | Authoritative product behavior, 18 illustrated pages, and the example that can go on paper. |

The private [idea-map prototype](https://fieldbook-idea-studio.plainalgae4.chatgpt.site/) demonstrates editing, recovery and paper workflows using browser-local data. Credentials, family records and a deployed production service are unnecessary for this handoff. The product evidence gates begin open. The synthetic fixture contains six base moments, a separate shareable highlight and four short, real sample files.
