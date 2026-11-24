# Model choice and working budget

Use the lightest available Codex model that can pass the specific ticket’s checks. The names below are examples from OpenAI’s current guidance. The model picker, entitlement and reasoning settings depend on the product and account. Assign by role if the exact name is unavailable. This is a proposed division of work for this family project.

| Assignment | Starting choice | When the job changes |
| --- | --- | --- |
| One deterministic function, small Svelte component, example data or existing test pattern | **GPT-6 Luna**, low or medium when available | Run the concrete example and counterexample. Pass the actual failing input to an integration model if a second focused attempt produces the same failure. |
| Rust handler plus persistence, schema migration, shared DTO, multi-tab conflicts or multiple layers | **GPT-6.2 Sol**, medium | Split out the one hard boundary first. Give remaining UI or fixture work back to the smaller model. |
| Device/visual check | A person with the specified device, or **GPT-6.1 Terra** with browser access for a documented simulated check | Keep physical Android, passkey, printer and child usability observations labeled separately from simulation. |
| Conflicting access rules, recovery after a later revocation or unresolved isolation bug | Independent Sol reviewer with the failing case, then an experienced human or **GPT-6.2 Astra** if the design decision remains open | Resolve the contract, rerun the adverse case and return implementation to a smaller model. A guardian reviews privacy-sensitive code before using family records. |

## Keep the prompt short

Give a builder its slice brief, a fixture, an interface and the two observations to demonstrate. Bring in a design section only when the brief refers to it. Ask for exactly the changed files and executable checks. Give the reviewer the candidate commit and the same fixture in a fresh session. An independent reader must be able to reproduce a failure with the information in the handback.

Calibrate locally on B12.1 (deterministic Rust), B15.1 (conflict/retry), and B23.2 (print layout). Record model, reasoning setting, first-pass test result, number of fixes and reviewer minutes. Keep using the smaller model when it passes the same checks at lower total effort. No cost, access tier or cross-model benchmark is assumed. Model availability as of the handoff is separate from the product’s optional outside-AI provider feature.

## Official references checked for this packet

- OpenAI describes Luna for bounded edits, Sol for coding and multi-step work, and Terra for visual work. [Model-selection guide](https://developers.openai.com/api/docs/guides/model-selection)
- Codex can load repository instructions and accept non-interactive prompts. The first assignment can also be pasted into the Codex interface. [Repository instructions](https://developers.openai.com/codex/guides/agents-md/) · [Non-interactive mode](https://developers.openai.com/codex/noninteractive/)
- For the actual passkey and household policy implementation, consult the production spec and maintained WebAuthn library documentation. Drive access requires a separate scoped design if the family later requests a connector. [Server-side passkey authentication](https://developers.google.com/identity/passkeys/developer-guides/server-authentication) · [Google Drive scopes](https://developers.google.com/workspace/drive/api/guides/api-specific-auth)
- Verify operator procedures against the selected database major version. [PostgreSQL 16 backup and restore](https://www.postgresql.org/docs/16/backup.html)
