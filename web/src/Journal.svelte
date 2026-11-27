<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import MomentDetails from './MomentDetails.svelte';
  import FamilyPhotos from './FamilyPhotos.svelte';
  import { api, RequestError, type Session } from './identity';
  import { attribution, eventDate, type Learner, type Moment, type MomentDraft } from './journal';

  interface Draft {
    submission: string;
    note: string;
    activityDate: string;
    scribed: boolean;
  }

  const {
    session, panel, entryMoment, onvisited, onjournal, onauthlost,
  }: {
    session: Session;
    panel: 'capture' | 'journal' | 'photos' | null;
    entryMoment: string | null;
    onvisited: () => void;
    onjournal: () => void;
    onauthlost: () => void;
  } = $props();

  let learners = $state<Learner[] | null>(null);
  let learnerId = $state('');
  let drafts = $state<Record<string, Draft>>({});
  let momentDrafts = $state<Record<string, MomentDraft>>({});
  let entries = $state<Moment[] | null>(null);
  let selected = $state<Moment | null>(null);
  let error = $state('');
  let status = $state('');
  let saving = $state(false);
  let sentence = $state<HTMLTextAreaElement>();
  let chooser = $state<HTMLSelectElement>();
  let heading = $state<HTMLHeadingElement>();
  let alive = true;

  let target = $derived(session.actor.role === 'learner' ? session.actor.id : learnerId);
  let draft = $derived(drafts[target]);
  let guardian = $derived(session.actor.role === 'guardian');

  onDestroy(() => {
    alive = false;
  });

  $effect(() => {
    const visiblePanel = panel;
    if (visiblePanel) void tick().then(() => show(visiblePanel));
  });

  async function show(view: 'capture' | 'journal' | 'photos') {
    if (!alive || panel !== view) return;
    error = '';
    if (view === 'journal') {
      heading?.focus();
      try {
        const loaded = await api<Moment[]>('/moments', 'GET', undefined, session.csrf_token);
        if (alive && panel === view) {
          entries = loaded;
          if (selected) selected = loaded.find((moment) => moment.id === selected?.id) ?? null;
          if (entryMoment) {
            const entry = loaded.find((moment) => moment.id === entryMoment);
            onvisited();
            if (entry) await showEntry(entry);
            else error = 'This moment is unavailable.';
          }
        }
      } catch (failure) {
        reportError(failure, 'The family journal could not load. Open it again to retry.');
      }
      return;
    }
    if (view === 'photos') heading?.focus();
    if (guardian && learners === null) {
      try {
        const loaded = await api<Learner[]>('/journal/learners', 'GET', undefined, session.csrf_token);
        if (!alive) return;
        learners = loaded;
        if (loaded.length === 1) learnerId = loaded[0].id;
      } catch (failure) {
        reportError(failure, 'The learner list could not load. Open capture again to retry.');
      }
    }
    await tick();
    if (alive && panel === view && view === 'capture') {
      if (target) sentence?.focus();
      else chooser?.focus();
    }
  }

  function reportError(failure: unknown, fallback: string) {
    if (!alive) return;
    if (failure instanceof RequestError) {
      if (failure.status === 401 || failure.status === 403) {
        onauthlost();
        return;
      }
      if (failure.status === 422) {
        const body = failure.body as { error?: string } | null;
        error = body?.error === 'note_or_asset_required'
          ? 'Write a sentence before saving.'
          : body?.error === 'invalid_activity_date'
            ? 'Enter a valid activity date.'
            : 'Check your sentence and try again.';
        return;
      }
      if (failure.status === 409) {
        error = 'This draft may already have been saved. Check the family journal before trying again.';
        return;
      }
    }
    error = fallback;
  }

  function newDraft(): Draft {
    return { submission: crypto.randomUUID(), note: '', activityDate: '', scribed: false };
  }

  function changeDraft(change: Partial<Draft>) {
    drafts[target] = { ...(draft ?? newDraft()), ...change };
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!target || saving) return;
    const written = draft ?? newDraft();
    const submittedTo = target;
    drafts[submittedTo] = written;
    error = '';
    status = '';
    saving = true;
    try {
      const moment = await api<Moment>('/moments', 'POST', {
        submission_key: written.submission,
        learner_id: submittedTo,
        note: written.note,
        activity_date: written.activityDate || null,
        scribed: written.scribed,
      }, session.csrf_token);
      if (!alive) return;
      delete drafts[submittedTo];
      if (panel === 'capture' && target === submittedTo) {
        selectMoment(moment);
        status = 'Moment saved.';
        onjournal();
      }
    } catch (failure) {
      if (panel === 'capture' && target === submittedTo) {
        reportError(failure, 'The moment could not be saved. Your draft is here to retry.');
      }
    } finally {
      saving = false;
    }
  }

  function chooseLearner(value: string) {
    learnerId = value;
    error = '';
  }

  function selectMoment(moment: Moment) {
    momentDrafts[moment.id] ??= { comment: '', kind: 'comment', submission: crypto.randomUUID() };
    selected = moment;
  }

  async function showEntry(moment: Moment) {
    error = '';
    try {
      const current = await api<Moment>(`/moments/${moment.id}`, 'GET', undefined, session.csrf_token);
      if (!alive || panel !== 'journal') return;
      selectMoment(current);
      status = '';
      await tick();
      heading?.focus();
    } catch (failure) {
      reportError(failure, 'The moment could not load.');
    }
  }

  function updateMoment(moment: Moment) {
    selected = moment;
    entries = entries?.map((entry) => entry.id === moment.id ? moment : entry) ?? null;
  }
</script>

{#if panel === 'capture'}
  <h1>Write a sentence</h1>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if guardian && learners === null && !error}<p role="status">Loading learners…</p>{/if}
  {#if guardian && learners?.length === 0}<p>Add a learner account before recording a moment.</p>{/if}
  <form onsubmit={save}>
    {#if guardian && (learners?.length ?? 0) > 1}
      <label for="journal-learner">Learner</label>
      <select id="journal-learner" bind:this={chooser} value={learnerId}
        onchange={(event) => chooseLearner(event.currentTarget.value)} disabled={saving}>
        <option value="">Choose a learner</option>
        {#each learners ?? [] as learner (learner.id)}
          <option value={learner.id}>{learner.display_name}</option>
        {/each}
      </select>
    {/if}
    {#if target}
      <label for="sentence">What happened?</label>
      <textarea id="sentence" rows="5" bind:this={sentence} value={draft?.note ?? ''}
        oninput={(event) => changeDraft({ note: event.currentTarget.value })}
        autocomplete="off" disabled={saving}></textarea>
      <label for="activity-date">Activity date (optional)</label>
      <input id="activity-date" type="date" value={draft?.activityDate ?? ''}
        oninput={(event) => changeDraft({ activityDate: event.currentTarget.value })} disabled={saving} />
      {#if guardian}
        <label class="check">
          <input type="checkbox" checked={draft?.scribed ?? false}
            onchange={(event) => changeDraft({ scribed: event.currentTarget.checked })}
            disabled={saving} />
          These are the learner’s spoken words
        </label>
        <p class="hint">{draft?.scribed ? 'The learner can correct or confirm these words.' : 'This will be saved as your observation.'}</p>
      {/if}
      <div class="actions">
        <button type="submit" disabled={saving}>Save moment</button>
        {#if draft?.note || draft?.activityDate || draft?.scribed}
          <button type="button" class="quiet" disabled={saving}
            onclick={() => { delete drafts[target]; error = ''; sentence?.focus(); }}>Discard draft</button>
        {/if}
      </div>
    {/if}
  </form>
{:else if panel === 'photos'}
  <h1 bind:this={heading} tabindex="-1">Family photos to match</h1>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if guardian && learners === null && !error}<p role="status">Loading learners…</p>{/if}
  {#if guardian && learners?.length === 0}<p>Add a learner account before saving a family photo.</p>{/if}
  {#if guardian && (learners?.length ?? 0) > 1}
    <label for="photo-learner">Learner</label>
    <select id="photo-learner" value={learnerId} onchange={(event) => chooseLearner(event.currentTarget.value)}>
      <option value="">Choose a learner</option>
      {#each learners ?? [] as learner (learner.id)}
        <option value={learner.id}>{learner.display_name}</option>
      {/each}
    </select>
  {/if}
  {#if target}
    {#key target}
      <FamilyPhotos {session} learner={target} {onauthlost} />
    {/key}
  {/if}
{:else if panel === 'journal'}
  {#if selected}
    <h1 bind:this={heading} tabindex="-1">Moment</h1>
    {#if status}<p role="status">{status}</p>{/if}
    {#key selected.id}
      <MomentDetails {session} moment={selected} draft={momentDrafts[selected.id]}
        {onauthlost} onupdate={updateMoment} />
    {/key}
    <button type="button" class="quiet" onclick={async () => {
      selected = null; status = ''; await tick(); await show('journal');
    }}>Back to family journal</button>
  {:else}
    <h1 bind:this={heading} tabindex="-1">Family journal</h1>
    {#if error}<p class="error" role="alert">{error}</p>
    {:else if entries === null}<p role="status">Loading the family journal…</p>
    {:else if entries.length === 0}<p>No moments yet.</p>
    {:else}
      <ul>
        {#each entries as moment (moment.id)}
          <li>
            <button type="button" class="quiet entry" onclick={() => showEntry(moment)}>
              <span>{moment.note || 'Moment with an attachment'}</span>
              <small>{attribution(moment)} · {eventDate(moment)}</small>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
{/if}

<style>
  h1 { font: 700 clamp(1.75rem, 5vw, 2.5rem)/1.15 Georgia, serif; margin: 0 0 1rem; }
  label { display: block; font-weight: 650; margin: .5rem 0; }
  input[type="date"] { max-width: 16rem; }
  .check { display: flex; align-items: center; gap: .5rem; min-height: 44px; }
  .check input { width: 1.25rem; min-height: 1.25rem; padding: 0; }
  .actions { display: flex; flex-wrap: wrap; gap: .5rem; margin-top: 1rem; }
  .hint { color: #483f34; }
  .error { color: #8b2620; }
  ul { margin: 0; padding: 0; list-style: none; display: grid; gap: .6rem; }
  .entry { text-align: left; width: 100%; overflow-wrap: anywhere; }
  .entry span, .entry small { display: block; }
</style>
