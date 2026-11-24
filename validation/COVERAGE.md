# Requirement and acceptance index

Each B number and T number is identical to the design release. A scenario can run through several features. The packet tests are examples inside a larger product acceptance case.

| Build and acceptance | Scenario coverage | Executable examples |
| --- | --- | --- |
| [B0 · Deployment skeleton](../tickets/B0.md) / T0 | S10 | Ticket-specific integration, device or operator rehearsal |
| [B1 · Signed-in notebook](../tickets/B1.md) / T1 | S1, S14, S15 | Ticket-specific integration, device or operator rehearsal |
| [B2 · Journal](../tickets/B2.md) / T2 | S1, S2, S3, S6, S11 | journal-retry |
| [B3 · Attachments](../tickets/B3.md) / T3 | S12, S13, S15 | photo-match |
| [B4 · Private diary](../tickets/B4.md) / T4 | S14 | diary-scope |
| [B5 · Relatives and outside feedback](../tickets/B5.md) / T5 | S3, S8, S12, S14 | guest-revoked |
| [B6 · Projects and evidence](../tickets/B6.md) / T6 | S2, S4, S6, S10, S11, S12, S13 | Ticket-specific integration, device or operator rehearsal |
| [B7 · Built-in Explore](../tickets/B7.md) / T7 | S4, S5, S13 | Ticket-specific integration, device or operator rehearsal |
| [B8 · Optional outside help](../tickets/B8.md) / T8 | S5, S13, S14 | ai-cancel |
| [B9 · Keepsakes](../tickets/B9.md) / T9 | S7, S8, S9 | Ticket-specific integration, device or operator rehearsal |
| [B10 · Long-term recovery](../tickets/B10.md) / T10 | S10, S14 | restore-corrupt |
| [B11 · Hosting rehearsal](../tickets/B11.md) / T11 | S10 | Ticket-specific integration, device or operator rehearsal |
| [B12 · Idea cards](../tickets/B12.md) / T12 | S16, S17 | lens-match, lens-dismissed, lens-new-field, lens-named |
| [B13 · Map and List](../tickets/B13.md) / T13 | S16, S17, S22 | Ticket-specific integration, device or operator rehearsal |
| [B14 · Small experiments](../tickets/B14.md) / T14 | S5, S8, S13, S16, S23 | Ticket-specific integration, device or operator rehearsal |
| [B15 · Ordered idea outline](../tickets/B15.md) / T15 | S18, S20, S22 | split-retry |
| [B16 · Input/output agreements](../tickets/B16.md) / T16 | S19, S22 | required-cycle |
| [B17 · Independent and combined checks](../tickets/B17.md) / T17 | S19, S22, S23 | sample-v-actual |
| [B18 · Ordered writing](../tickets/B18.md) / T18 | S20 | essay-legacy |
| [B19 · Connected project graph](../tickets/B19.md) / T19 | S18, S19, S22 | graph-privacy |
| [B20 · Past and new achievements](../tickets/B20.md) / T20 | S7, S9, S12, S21, S23 | portfolio-withdraw |
| [B21 · Recovery for every artifact](../tickets/B21.md) / T21 | S10, S18, S19, S22 | Ticket-specific integration, device or operator rehearsal |
| [B22 · Permissions after recovery](../tickets/B22.md) / T22 | S10, S14, S21 | restore-revoked |
| [B23 · Paper worksheet](../tickets/B23.md) / T23 | S18, S24 | print-branch, paper-retry |
| [B24 · Pretotypes and cumulative prototypes](../tickets/B24.md) / T24 | S18, S25 | trials-many-builds, trials-atomic-retry, trials-version-pins |
| [B25 · Project Failure Résumé and revival](../tickets/B25.md) / T25 | S23, S24, S26 | resume-project-print, resume-revival, resume-legacy-empty |

## Family scenarios

| ID | Situation | Build tickets |
| --- | --- | --- |
| S1 | Five minutes after teaching fractions | B1, B2 |
| S2 | A robot turns too far | B2, B6 |
| S3 | A teacher, friend, or parent notices something | B2, B5 |
| S4 | The habit of testing travels | B6, B7 |
| S5 | Friday is busy, or he hit a dead end | B7, B8, B14 |
| S6 | Family learning outside STEM | B2, B6 |
| S7 | A term ends | B9, B20 |
| S8 | A mentor offers a short conversation | B5, B9, B14 |
| S9 | A later scholarship, internship, research program, or college asks for a sample | B9, B20 |
| S10 | Interests change, or the family moves the app | B0, B6, B10, B11, B21, B22 |
| S11 | A club visit, Scout outing or family memory | B2, B6 |
| S12 | A sports or academic tournament | B3, B5, B6, B20 |
| S13 | An AI-assisted game or invention | B3, B6, B7, B8, B14 |
| S14 | A private reflection and a selected audience | B1, B4, B5, B8, B10, B22 |
| S15 | The photo and words arrive on different days | B1, B3 |
| S16 | One idea becomes a different kind of project | B12, B13, B14 |
| S17 | Anush builds a part of his own Fieldbook | B12, B13 |
| S18 | Anush has an ambitious project | B15, B19, B21, B23, B24 |
| S19 | Two parts can be built separately | B16, B17, B19, B21 |
| S20 | A persuasive essay grows from a club experience | B15, B18 |
| S21 | A proud moment predates the app | B20, B22 |
| S22 | Music, math and coding share one project | B13, B15, B16, B17, B19, B21 |
| S23 | Today’s experiment becomes a new achievement | B14, B17, B20, B25 |
| S24 | Anush will be away from the computer | B23, B25 |
| S25 | Several small tests guide two builds | B24 |
| S26 | A failed attempt becomes a point of pride | B25 |
