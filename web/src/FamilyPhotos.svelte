<script lang="ts">
  import { onMount, tick } from 'svelte';
  import PhotoPicker from './PhotoPicker.svelte';
  import { api, RequestError, type Session } from './identity';
  import { inbox, matchPath, type Attachment } from './photos';

  const { session, learner, moment, onauthlost }: {
    session: Session;
    learner: string;
    moment?: string;
    onauthlost: () => void;
  } = $props();
  let waiting = $state<Attachment[]>([]);
  let attached = $state<Attachment[]>([]);
  let loading = $state(true);
  let pending = $state(false);
  let error = $state('');
  let status = $state('');
  let title = $state<HTMLHeadingElement>();
  let canMatch = $derived(session.actor.role === 'guardian' || session.actor.id === learner);
  const headingId = $props.id();

  onMount(() => { void load(); });

  function sessionError(failure: unknown) {
    if (failure instanceof RequestError && [401, 403].includes(failure.status)) {
      onauthlost();
      return true;
    }
    return false;
  }

  async function load() {
    loading = true;
    error = '';
    try {
      [waiting, attached] = await Promise.all([
        canMatch ? api<Attachment[]>(`${inbox}/${learner}`, 'GET', undefined, session.csrf_token) : [],
        moment ? api<Attachment[]>(`/moments/${moment}/attachments`, 'GET', undefined, session.csrf_token) : [],
      ]);
    } catch (failure) {
      if (!sessionError(failure)) error = 'The family files could not load.';
    } finally {
      loading = false;
    }
  }

  async function change(item: Attachment, matched: boolean) {
    if (!moment || pending) return;
    pending = true;
    error = '';
    status = '';
    try {
      await api<void>(matchPath(item.id, moment), matched ? 'POST' : 'DELETE', undefined, session.csrf_token);
      status = matched ? 'File matched.' : 'File returned to Family photos to match.';
      await load();
      await tick();
      title?.focus();
    } catch (failure) {
      if (sessionError(failure)) return;
      if (failure instanceof RequestError && failure.status === 409) {
        await load();
        error = 'This file is already matched to another moment.';
      } else if (failure instanceof RequestError && failure.status === 422) {
        error = matched ? 'This moment already has 20 attachments.'
          : 'Write a sentence before removing the last attachment.';
      } else {
        error = 'The match could not be changed. Try again.';
      }
    } finally {
      pending = false;
    }
  }
</script>

{#snippet label(item: Attachment)}
  <span class="filename">{item.label}</span>
  <small>Uploaded by {item.uploader_name}{item.activity_date ? ` · ${item.activity_date}` : ''}</small>
{/snippet}

{#if moment}
  <h2 bind:this={title} tabindex="-1">Attachments</h2>
  {#if attached.length}
    <ul>
      {#each attached as item (item.id)}
        <li>
          {@render label(item)}
          {#if item.matched_by_name}<small>Matched by {item.matched_by_name}</small>{/if}
          {#if item.from_inbox && canMatch}
            <button type="button" class="quiet" aria-label={`Unmatch ${item.label}`}
              disabled={pending} onclick={() => change(item, false)}>Unmatch</button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if !loading && !error}
    <p>No attachments yet.</p>
  {/if}
{/if}

{#if session.actor.role === 'guardian'}
  <PhotoPicker {session} {learner} {moment} {onauthlost} onsaved={() => { void load(); }} />
{/if}

{#if canMatch}
  <section aria-labelledby={headingId}>
    {#if moment}<h3 id={headingId}>Family photos to match</h3>
    {:else}<h2 id={headingId}>Waiting to match</h2>{/if}
    {#if waiting.length}
      <ul>
        {#each waiting as item (item.id)}
          <li>
            {@render label(item)}
            {#if moment}
              <button type="button" class="quiet" aria-label={`Match ${item.label}`}
                disabled={pending} onclick={() => change(item, true)}>Match</button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if !moment}<p>Open a family moment to match a file.</p>{/if}
    {:else if !loading && !error}
      <p>No files waiting to match.</p>
    {/if}
  </section>
{/if}
{#if loading}<p role="status">Loading family files…</p>{/if}
{#if status}<p role="status">{status}</p>{/if}
{#if error}
  <p class="error" role="alert">{error}</p>
  <button type="button" class="quiet" disabled={pending} onclick={load}>Reload family files</button>
{/if}

<style>
  ul { padding-left: 1.25rem; }
  li { margin: .625rem 0; }
  small, .filename { display: block; overflow-wrap: anywhere; }
  li button { margin-top: .25rem; }
  .error { color: #8b2620; }
</style>
