# Test data the family can replace

All people, roles, events and file bytes in these fixtures are invented. The separate reference design uses the family’s chosen project language. The role aliases in the fixture are the ones to seed in a disposable application environment.

| Input | Expected use |
| --- | --- |
| `household.json` | Six primary moments in household A, one explicit highlight, one frozen page, two household-A guardians, one named relative and an unrelated household B. One active grant selects only a photo; a distinct grant for the letter has been withdrawn. |
| `assets.json` and `media/` | Four genuine small originals, including a readable sample PDF, geometric JPG, ZIP project and playable one-second MP4. The full recording is a fifth, link-only item in the household fixture. Use the supplied size and SHA-256. |
| `project.json` | A fictional neighborhood seed exchange with ordered ideas, stable paper codes, collapsed children, internal and out-of-scope handoffs, and unselected text markers. Seed it as a separate project so the baseline six-moment count remains stable. |
| `lens.json` and `beat-pattern.json` | Pure-rule input for selection, a versioned producer/consumer example, and a deliberate indexing mismatch. |
| `project-trials.json` | Three observed pretotypes, one pending draft, one different project and a pre-trials legacy project. The six B24/B25 vectors test success, rejection, historical identity and print selection. |
| `archive-vectors.json` | Semantic migration expectations for schema 1–7. Real importer compatibility requires actual historical archive bytes. |
| `../contracts/planning-examples.json` | UUID-backed worked REST examples and symbolic aliases for assertions on order and identity. |
| `../validation/cases.json` | Per-case inputs and precise normalized observable outputs for a test-only adapter. |

Use a clean database or isolated namespace per case. Read actual IDs, counts, file hashes and visible content after driving the production code. Scale performance fixtures separately to 1,000 moments and 2,000 nodes. For a multipage sheet make at least 90 ordered test rows, fold and unfold a branch, and retain its printed references.
