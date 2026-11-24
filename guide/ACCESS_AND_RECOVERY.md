# Household access, reversibility and durability

Use the ordinary application commands with the invented household in `fixtures/household.json`. Pair one authorized read or write with an ordinary role that has a different scope. Test original and preview, UI and API, relation IDs and counts. The product spec’s access and recovery rules govern this table.

| Actor | Family journal and plan | Diary | Selected share or files |
| --- | --- | --- | --- |
| Learner | Write own words, respond to feedback, link evidence and choose highlights | Own diary and parental access history | Preview precisely the text and representations selected |
| Guardian, ordinary session | Add attributed contributions, original files or comments and manage family settings | No normal diary contents, counts, search results or thumbnails | Review a named guest and approved items |
| Either guardian, freshly elevated | Normal family actions stay in their ordinary context | Time-limited, read-only access for this learner. Write durable access event before revealing content | No coaching, copying to the family graph or sharing from the elevated screen |
| Named invited relative | Only the selected family entry/highlight and allowed comments | No access | Only the approved revision and representation, rechecked for every read and download |
| Other household | No access to this family’s IDs | No access | Same missing-object response for a signed-in unauthorized caller |
| Backup operator | Encrypted snapshots, manifest, recovery and account re-enrollment | Diary encrypted separately inside a protected household archive. Reading through the restored app requires fresh logged access | Apply later revocations before reopening. Restore does not revive sessions or stale grants. |

## Reversible commands to exercise

| Change | Expected inverse and observable preservation |
| --- | --- |
| Create, trash and restore a project or a journal moment | Recover the same IDs, original author, history and ordered relations. Trashing the final project gives an empty state. |
| Child, split, insert, reparent or reorder | Select the child; insert between neighbors; split it again. Undo/redo as one revision. Preserve unrelated edits. Offer delete-branch or keep-children choices. |
| Connect a handoff and record a check | Disconnect, recover the handoff, then recover its check. Preserve pinned revisions. A missing endpoint or occupied input stays pending with a reason. |
| Attach or match a source | Detach and reattach a relation using the original asset. Unmatch a photo back to its inbox. Clearing a collection leaves separately owned originals. |
| Select a portfolio item, comment, tag or source | Withdraw and explicitly reselect if currently authorized. Authorship, the originating moment, files and other project links survive. |
| Share, elevate or publish | Revoke a named grant, end a session or withdraw future access. A new share needs new approval. Retain historical audit events and correct them by appending. |
| Capture, park, feature and revive a trial | Restore a parked trial, unfeature and refeature a source. Keep dated predictions and results. A revival adds its own trial and changed condition. Authorize both ends for each reader and renderer. |
| Print and enter paper marks | Complete, reopen, undo and redo. Search by stable code after a rename or reorder. A planning mark creates no observation, portfolio item or passed handoff. |

## Storage and account boundary

Use independent household accounts with passkeys or a user-verifying security key on the family computer. On Android a guardian selects a photo or PDF using the standard picker. Long recordings may remain links. A verified archived copy stores the original bytes, immutable key, length, SHA-256, creator and uploader. Sharing the selected photo does not include the teacher letter sitting beside it. Open a restricted Google Doc by an explicit click. A stored URL is a pointer; the backend does not fetch it. Google Cloud hosting does not require child Google Sign-In or broad Drive access.

The production archive schema is **7**. The prototype’s browser project JSON version, REST prefix `/api/v1`, design v0.8 and handoff v0.8.1 evolve separately. A browser export is a single project and is unsuitable as a household restore test. `fixtures/archive-vectors.json` gives expectations for schema 1–7. Generate authentic serialized/encrypted fixtures from the matching historical implementation or retain a reviewed golden archive before claiming parser compatibility. Use ephemeral test recovery material; retain no keys in tickets, logs or source control.

For a clean restore, stage a consistent SQL snapshot and exactly its referenced media, publish a completed manifest last, verify every byte, reconcile the latest consent/revocation journal, and activate one destination. Diff original IDs, asset SHA-256, audit actors, file readability and denied guest scope. A corrupted copy leaves both source and prior good backup intact. A rollback after target writes starts by draining and exporting those newer writes. See design §§7 and 9 for the recipes.

Schema 7 keeps project trial IDs, immutable observation revisions, prototype sources, previous-version links, résumé selections and revivals. Restore versions 1–6 with empty trial collections.
