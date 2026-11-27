<script lang="ts">
  import { onMount } from 'svelte';
  import { api, RequestError, type DiaryAccessKind, type Session } from './identity';
  import type { Learner } from './journal';

  const { session, busy, onopen, onauthlost }: {
    session: Session;
    busy: boolean;
    onopen: (learner: string, kind: DiaryAccessKind) => void;
    onauthlost: () => void;
  } = $props();
  let heading: HTMLHeadingElement;
  let learners = $state<Learner[] | null>(null);
  let learner = $state('');
  let kind = $state<DiaryAccessKind>('view');
  let failed = $state(false);
  let active = false;

  onMount(() => {
    active = true;
    heading.focus();
    void load();
    return () => { active = false; };
  });

  async function load() {
    failed = false;
    try {
      const loaded = await api<Learner[]>('/journal/learners', 'GET', undefined, session.csrf_token);
      if (!active) return;
      learners = loaded;
      if (loaded.length === 1) learner = loaded[0].id;
    } catch (failure) {
      if (!active) return;
      if (failure instanceof RequestError && [401, 403, 404].includes(failure.status)) onauthlost();
      else failed = true;
    }
  }
</script>

<h1 bind:this={heading} tabindex="-1">Parent access</h1>
<p>The learner can see your name and when you opened their diary. This is read-only access for up to five minutes.</p>
{#if failed}
  <p role="alert">The learner list could not load.</p>
  <button type="button" class="quiet" onclick={load}>Try again</button>
{:else if learners === null}
  <p role="status">Loading learners…</p>
{:else if learners.length === 0}
  <p>No learner accounts are available.</p>
{:else}
  <form onsubmit={(event) => { event.preventDefault(); if (learner && !busy) onopen(learner, kind); }}>
    <label for="parent-access-learner">Learner</label>
    <select id="parent-access-learner" bind:value={learner} required disabled={busy}>
      {#if learners.length > 1}<option value="">Choose a learner</option>{/if}
      {#each learners as person (person.id)}
        <option value={person.id}>{person.display_name}</option>
      {/each}
    </select>
    <label for="parent-access-kind">Reason</label>
    <select id="parent-access-kind" bind:value={kind} disabled={busy}>
      <option value="view">View diary</option>
      <option value="recovery">Recovery</option>
    </select>
    <button type="submit" disabled={busy || !learner}>Open with your passkey</button>
  </form>
{/if}

<style>
  h1 { font: 700 clamp(1.75rem, 5vw, 2.5rem)/1.15 Georgia, serif; margin: 0 0 1rem; }
  label { display: block; font-weight: 650; margin: .5rem 0; }
  form > button { margin-top: 1rem; }
</style>
