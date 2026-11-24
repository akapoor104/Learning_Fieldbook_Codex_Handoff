# Anush’s first Rust assignment

**Goal for one 45-minute session.** Given a goal about grouping and two possible borrowed methods, return the rhythm card. When that card is dismissed, return no suggestion. Anush can do this on the family computer using made-up records.

1. **Five minutes.** Open `fixtures/lens.json`. Point to the current field, mechanism, time limit and materials. Predict which card the program will choose. Explain why a trail sign is useful for a different goal.
2. **Twenty minutes.** In a Rust crate in the production or practice checkout, define `Focus`, `Lens` and a `choose_lens` function over input values. Filter by a different field, shared mechanism, available materials, time, requested field and focus-specific dismissed/kept version. Among available fields, prefer the least recently kept. Break ties by stable card ID. Return an optional existing card. Start with two unit tests.
3. **Ten minutes.** Add one navigation case and one named-field case. Run the focused tests with Cargo. If the production test adapter is ready, run the four supplied B12 vectors through that Rust adapter. A new version of a card can be considered after a dismissal of its older version.
4. **Ten minutes.** Explain one decision aloud. Try a new domain of your own using a data row. Use B12.2 in a later session to reveal the bridge, limitation and one small test with a keyboard-accessible Svelte button.

Copy-ready prompt for a small model:

> Coach Anush through B12.1 using fixtures/lens.json. Let him predict the first result before coding. Work only on the pure Rust selection rule and its tests. Show why the rhythm option was selected and how the function behaves after dismissal. Use synthetic inputs, zero network calls, and no database, accounts or frontend. End with the command Anush can run again and one question asking him to explain the rule.

The packet includes a **working Python demonstration** of the same four inputs so the family can see the desired behavior immediately. The production assignment uses Rust.

```sh
python3 tools/run_cases.py --ticket B12 -- python3 examples/lens_reference.py
```
