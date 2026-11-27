<script lang="ts">
  import { onMount } from 'svelte';
  import { api, RequestError, type Session } from './identity';
  import { eventDate, type Moment } from './journal';

  const { session, onauthlost }: { session: Session; onauthlost: () => void } = $props();
  let heading: HTMLHeadingElement;
  let entries = $state<Moment[] | null>(null);
  let failed = $state(false);

  onMount(() => {
    let active = true;
    heading.focus();
    void api<Moment[]>('/diary', 'GET', undefined, session.csrf_token).then((loaded) => {
      if (active) entries = loaded;
    }).catch((failure: unknown) => {
      if (!active) return;
      if (failure instanceof RequestError && (failure.status === 401 || failure.status === 403)) {
        onauthlost();
      } else {
        failed = true;
      }
    });
    return () => { active = false; };
  });
</script>

<h1 bind:this={heading} tabindex="-1">My diary</h1>
{#if failed}
  <p role="alert">Your diary could not load. Open it again to retry.</p>
{:else if entries === null}
  <p role="status">Loading your diary…</p>
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
  article { margin-block: 1rem; }
  h2 { font-size: 1rem; }
  article p { white-space: pre-wrap; overflow-wrap: anywhere; }
  article a { display: block; width: fit-content; }
</style>
