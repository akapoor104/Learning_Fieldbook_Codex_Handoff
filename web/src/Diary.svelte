<script lang="ts">
  import { onMount } from 'svelte';
  import { api, RequestError, type DiaryAccessKind, type Session } from './identity';
  import { eventDate, type Moment } from './journal';

  interface AccessEvent {
    id: string;
    guardian_name: string;
    kind: DiaryAccessKind;
    recorded_at: string;
  }

  const { session, onauthlost }: { session: Session; onauthlost: () => void } = $props();
  const parent = $derived(session.actor.role === 'guardian');
  const eventTime = new Intl.DateTimeFormat('en-US', {
    timeZone: 'America/Los_Angeles', month: 'short', day: 'numeric', year: 'numeric',
    hour: 'numeric', minute: '2-digit', second: '2-digit', timeZoneName: 'short',
  });

  let heading: HTMLHeadingElement;
  let entries = $state<Moment[] | null>(null);
  let events = $state<AccessEvent[] | null>(null);
  let failed = $state(false);
  let historyFailed = $state(false);
  let active = false;

  onMount(() => {
    active = true;
    heading.focus();
    void loadDiary();
    if (!parent) void loadHistory();
    return () => { active = false; };
  });

  function checkAuth(failure: unknown) {
    if (failure instanceof RequestError
      && ([401, 403].includes(failure.status) || (parent && failure.status === 404))) {
      onauthlost();
      return true;
    }
    return false;
  }

  async function loadDiary() {
    failed = false;
    try {
      const loaded = await api<Moment[]>('/diary', 'GET', undefined, session.csrf_token);
      if (active) entries = loaded;
    } catch (failure) {
      if (active && !checkAuth(failure)) failed = true;
    }
  }

  async function loadHistory() {
    historyFailed = false;
    try {
      const loaded = await api<AccessEvent[]>('/diary/access-history', 'GET', undefined, session.csrf_token);
      if (active) events = loaded;
    } catch (failure) {
      if (active && !checkAuth(failure)) historyFailed = true;
    }
  }
</script>

<h1 bind:this={heading} tabindex="-1">{parent ? 'Learner’s diary' : 'My diary'}</h1>
{#if parent}
  <p>Read-only parent access. This view closes automatically. Sign out when you finish.</p>
{:else}
  <p>This diary is yours. If Mom or Dad needs to open it, you can see who opened it and when.</p>
  <details>
    <summary>Access history</summary>
    {#if historyFailed}
      <p role="alert">Access history could not load.</p>
      <button type="button" class="quiet" onclick={loadHistory}>Try again</button>
    {:else if events === null}
      <p role="status">Loading access history…</p>
    {:else if events.length === 0}
      <p>No parent access has been recorded.</p>
    {:else}
      <ul class="access-history">
        {#each events as event (event.id)}
          <li>
            <strong>{event.guardian_name}</strong>
            <span>{event.kind === 'recovery' ? 'Recovery' : 'Viewed diary'}</span>
            <time datetime={event.recorded_at}>{eventTime.format(new Date(event.recorded_at))}</time>
          </li>
        {/each}
      </ul>
    {/if}
  </details>
{/if}
{#if failed}
  <p role="alert">The diary could not load.</p>
  <button type="button" class="quiet" onclick={loadDiary}>Try again</button>
{:else if entries === null}
  <p role="status">Loading diary…</p>
{:else if entries.length === 0}
  <p>No diary entries yet.</p>
{:else}
  {#each entries as entry (entry.id)}
    <article>
      <h2>{eventDate(entry)}</h2>
      {#if entry.note}<p>{entry.note}</p>{/if}
      {#each entry.asset_ids as id, index (id)}
        <a href={`/diary/media/${id}`} download>Download attachment {index + 1}</a>
      {/each}
    </article>
  {/each}
{/if}

<style>
  h1 { font: 700 clamp(1.75rem, 5vw, 2.5rem)/1.15 Georgia, serif; margin: 0 0 1rem; }
  summary { cursor: pointer; font-weight: 650; width: fit-content; min-block-size: 44px; padding-block: .55rem; }
  .access-history { padding-left: 1.25rem; }
  .access-history li { margin-bottom: .75rem; }
  .access-history span, time { display: block; }
  article { margin-block: 1rem; }
  h2 { font-size: 1rem; }
  article p { white-space: pre-wrap; overflow-wrap: anywhere; }
  article a { display: flex; align-items: center; min-block-size: 44px; width: fit-content; }
</style>
