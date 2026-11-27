<script lang="ts">
  import { onMount } from 'svelte';
  import { api, RequestError, type Session } from './identity';
  import SharedMaterial from './SharedMaterial.svelte';
  import ShareThread from './ShareThread.svelte';
  import type { SharedMoment } from './shares';

  let { session, onauthlost }: { session: Session; onauthlost: () => void } = $props();
  let items = $state<{ id: string; activity_date: string | null }[]>([]);
  let loaded = $state(false);
  let shown = $state<SharedMoment | null>(null);
  let code = $state('');
  let message = $state('');
  let error = $state('');
  let busy = $state(false);
  let reading = 0;
  let listing = 0;

  function sessionError(failure: unknown) {
    if (failure instanceof RequestError && [401, 403].includes(failure.status)) {
      onauthlost();
      return true;
    }
    return false;
  }

  async function reload() {
    const started = ++listing;
    const current = await api<typeof items>('/shares', 'GET', undefined, session.csrf_token);
    if (started !== listing) return;
    items = current;
    loaded = true;
    if (shown && !items.some((item) => item.id === shown?.id)) unavailable();
  }

  function unavailable() {
    reading++;
    shown = null;
    error = 'This share is unavailable.';
  }

  async function refresh() {
    try {
      await reload();
    } catch (failure) {
      if (!sessionError(failure)) error = 'Shared items could not load.';
    }
  }

  onMount(() => {
    void refresh();
    const id = /^\/shares\/([0-9a-f-]{36})\/$/i.exec(location.pathname)?.[1];
    if (id) void open(id);
    const timer = setInterval(() => { if (!document.hidden) void refresh(); }, 15000);
    return () => clearInterval(timer);
  });

  async function open(id: string) {
    const started = ++reading;
    shown = null;
    error = '';
    try {
      const item = await api<SharedMoment>(`/shares/${id}`, 'GET', undefined, session.csrf_token);
      if (started === reading) shown = item;
    } catch (failure) {
      if (started === reading && !sessionError(failure)) {
        unavailable();
        await refresh();
      }
    }
  }

  async function redeem(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    const [id, token, extra] = code.trim().split('.');
    message = '';
    error = '';
    if (!id || !token || extra !== undefined || !/^[0-9a-f-]{36}$/i.test(id)) {
      error = 'Enter the invitation code from your family.';
      return;
    }
    busy = true;
    try {
      await api(`/shares/${id}/accept`, 'POST', { token }, session.csrf_token);
      code = '';
      message = 'Invitation accepted.';
      await reload();
      await open(id);
    } catch (failure) {
      if (!sessionError(failure)) error = 'This invitation is unavailable.';
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onfocus={refresh} onpageshow={refresh} />

{#if error}<p role="alert">{error}</p>{/if}
{#if message}<p role="status">{message}</p>{/if}
{#if loaded && items.length === 0}<p>No shared items yet.</p>{/if}
{#if items.length}
  <h2>Shared items</h2>
  <ul aria-label="Shared items">
    {#each items as item (item.id)}
      <li><a href={`/shares/${item.id}/`}>
        Open shared moment{item.activity_date ? ` · ${item.activity_date}` : ''}
      </a></li>
    {/each}
  </ul>
{/if}
{#if shown}
  {#key shown.id}
    <h2>Shared moment</h2>
    <SharedMaterial item={shown} download />
    <ShareThread {session} id={shown.id} {onauthlost} onmissing={() => { unavailable(); void refresh(); }} />
  {/key}
{/if}
<form onsubmit={redeem}>
  <label>Relative invitation code <input bind:value={code} autocomplete="off" required /></label>
  <button type="submit" disabled={busy}>Accept invitation</button>
</form>
