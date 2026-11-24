# Learning Fieldbook workflow review

*Review of design v0.6 · November 19, 2026*

## Where the workflow gets in the way

The map already handles first ideas, splits, insertion, reordering and restoration. The first repair is to protect a thought while Anush is writing it. The next improvement is to let him use an observation in several places without entering it again.

### An open draft can disappear

I typed “Try a slower paper beat” into **Add sub-idea**. Selecting another card in a second tab closed the form. Reopening it under the original card showed an empty title.

Keep each tab’s selection separate from saved project edits. Preserve the writer’s draft. If the underlying idea changes, let the writer review both versions before saving.

### Journal evidence has to be typed again

The mock accepts one journal source note per map and asks for its title and excerpt. The full design already allows the same family moment to support multiple projects.

Add **Choose a saved moment** beside **Write a moment**. Keep a project’s starting moment and let it link later observations by ID. Unlinking a moment keeps its words, files and family comments.

### The last card viewed can displace the next step

The screen spec reopens the last selected card. The learning workflow promises a return to a chosen step or question. The mock remembers the selection.

Let Anush mark **Try next**. On Home, show **Continue** with that step’s name beside **Write a sentence**. Looking around the map should never change what he chose.

### Different goals get the same surprise

A fractions lesson, an overshooting robot and a creek essay each got “Make a trail of clues” as the first suggestion in the mock.

Apply the spec’s goal-matching rule to the built-in cards. Offer one borrowed method, why it fits, a limit and a small test. Keep **Different angle** and **My own idea** within reach.

### Recovery has two addresses

A deleted project is under **My projects → Trash**. A removed idea, connection or source is under **Removed items** inside its project.

Offer one **Recently removed** entry with project and item filters. Put **Restore** in the removal message. Keep **Undo** and **Redo** for recent changes within the open project.

### What was checked

The v0.6 design specification, 17-page walkthrough, working map and its 12 automated tests were reviewed. All 12 passed. The draft loss was reproduced in the browser. The three suggestion results came from the mock’s own generator. Family usability is still to be tested.

## One club moment, two devices

In the proposed flow, Anush can finish after the first step. The other steps become available when a parent has a photo, a question is worth trying or Anush wants to keep the result.

### 1. Anush writes at the shared computer

From **Write a sentence**, he enters “Once we lined up the strips, someone spotted that two quarters made a half.” He sees who can read it and chooses **Save moment**. The project, picture, skill and outcome can wait. **My diary** stays a separate, clearly marked choice.

### 2. A parent adds the photo from Android

The parent can upload to **Family photos** before or after the writing. Show **Uploading**, **Needs retry**, **Received in Family photos**, then **Linked to [moment]** as appropriate. Anush or a parent can match a family-visible moment or return the photo to the inbox. Keep the uploader and Anush’s words distinct.

### 3. Anush chooses something to try

From **Explore**, music suggests a way to hear equal groups. He marks “Tap four beats in two different groupings” as **Try next**. Home remembers it while he explores other cards. He can change or clear it. If he asks for encouragement, offer a note about his effort or a quotation with a verified source.

### 4. He records what happened

Suppose he writes “Both patterns ended together, but my friend counted taps instead of spaces.” **Record what happened** saves one new dated observation linked to the test. It can appear in Math club and Music. A parent can comment, and Anush can select or withdraw it from his portfolio later.

### Three different ways to relate ideas

| What Anush means | Control | What changes |
| --- | --- | --- |
| “This is part of my plan.” | Add a part | Place it in the outline. Move it up or down to change the sequence. |
| “This gave me an idea.” | Inspired by | Link the two ideas. Each can proceed independently. |
| “I need its result first.” | Use an output | Save the need as a draft. Agree on the output, input, example and pass condition before treating the handoff as ready. |

### For an essay

Moving a paragraph changes reading order. Only an explicit change to a handoff reconnects inputs or outputs. A passed sample and a successful trial with the real output stay distinct.

## What to build and test first

Use the existing Rust/Axum REST API design, Svelte frontend and PostgreSQL model. The browser mock keeps its data on one device. The two-device checks below belong in the signed-in family pilot.

### 1. Protect an unfinished thought

Type a sub-idea in tab A. Select a card in tab B. Tab A must keep the text and its original parent. Repeat with a real edit to that parent and resolve it explicitly. Switch accounts on the shared computer and verify that the next person cannot see the draft.

### 2. Save once across devices

Use a learner account on the computer and a guardian account on Android. Try words first and photo first. Interrupt and retry the upload. Finish with one moment, one original file and two correctly attributed contributions. An ordinary family picker must show zero private diary matches. Builds on B1–B4.

### 3. Follow the chosen step to a real result

Choose step A, look at B, then reopen Home. **Continue** opens A. Retry saving one observation and count one result. Link it to two projects, select it for Portfolio, then withdraw it. Both project links and the original observation survive. Builds on B7, B14 and B20.

### 4. Check what the family can get back

Attach a sample teacher PDF as a saved copy. Revoke a relative’s access, remove and restore the project, then restore the family archive on a fresh instance. The authorized family can open the identical PDF and see its source. The relative’s access stays revoked. Builds on B3, B10 and B21–B22.

### 5. Try the creative tools on different work

Ask for one angle on equal groups, a robot turn and an essay claim. A robot could borrow floor marks from stage choreography to see where it overshoots. Each suggestion needs a test Anush can explain or skip. Reorder an essay paragraph, then agree a robot handoff. Only the latter asks for an input/output agreement. Builds on B12 and B16–B18.

### Two weeks with the family

Keep the existing targets of independent capture within 90 seconds, finding an earlier example within two minutes, and an optional five-minute weekly check-in. Try math club plus an interest Anush picks. Add one proposed target of finding his chosen next step within 20 seconds. Note where he asks for help or leaves a field blank.

### One decision for the pilot

Home could show one next step Anush chooses across all interests, or one per active project. Start with one chosen step across interests. Each project keeps its own plan.

### Reviewed documents

[Design specification v0.6](https://chatgpt.com/library?file_id=libfile_d8d6471021788191a8558fb27ddfe72c&account_id=personal) · [Visual walkthrough v0.6](https://chatgpt.com/library?file_id=libfile_1f06a6fc4e10819188c26f5c48c6b65c&account_id=personal)
