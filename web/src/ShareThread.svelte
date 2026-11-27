<script lang="ts">
  import { onMount } from 'svelte';
  import { api, RequestError, type Session } from './identity';
  import { readableDate, type Comment } from './journal';

  let { session, id, onmissing, onauthlost }: {
    session: Session; id: string; onmissing: () => void; onauthlost: () => void;
  } = $props();

  let comments = $state<Comment[]>([]);
  let loaded = $state(false);
  let draft = $state('');
  let key = crypto.randomUUID();
  let busy = $state(false);
  let error = $state('');
  let path = $derived(`/shares/${id}/comments`);

  async function reload() {
    comments = await api<Comment[]>(path, 'GET', undefined, session.csrf_token);
    loaded = true;
  }

  function failed(failure: unknown) {
    if (failure instanceof RequestError && failure.status === 404) onmissing();
    else if (failure instanceof RequestError && [401, 403].includes(failure.status)) onauthlost();
    else error = 'The conversation could not be updated. Try again.';
  }

  async function update(event: SubmitEvent | null, comment?: Comment) {
    event?.preventDefault();
    if (busy) return;
    busy = true;
    error = '';
    try {
      if (comment) {
        const itemPath = `${path}/${comment.id}`;
        await api(comment.withdrawn ? `${itemPath}/restore` : itemPath,
          comment.withdrawn ? 'POST' : 'DELETE', undefined, session.csrf_token);
      } else {
        await api(path, 'POST', { body: draft, submission_key: key }, session.csrf_token);
        draft = '';
        key = crypto.randomUUID();
      }
      await reload();
    } catch (failure) {
      failed(failure);
    } finally {
      busy = false;
    }
  }

  onMount(() => { void reload().catch(failed); });
</script>

<h3>Shared conversation</h3>
{#if error}<p role="alert">{error}</p>{/if}
{#if loaded && comments.length === 0}<p>No comments yet.</p>{/if}
{#if comments.length}
  <ul aria-label="Shared comments">
    {#each comments as comment (comment.id)}
      <li>
        <p>{comment.author_name} · {readableDate(comment.created_at)}</p>
        <p>{comment.body}</p>
        {#if comment.withdrawn}<p>Comment withdrawn.</p>{/if}
        {#if comment.author_id === session.actor.id}
          <button type="button" class="quiet" disabled={busy} onclick={() => update(null, comment)}>
            {comment.withdrawn ? 'Restore shared comment' : 'Withdraw shared comment'}
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}
{#if loaded}
  <form onsubmit={update}>
    <label>Comment on this share <textarea rows="3" bind:value={draft} required disabled={busy}></textarea></label>
    <button type="submit" disabled={busy}>Post shared comment</button>
  </form>
{:else if error}
  <button type="button" onclick={() => reload().catch(failed)}>Reload comments</button>
{/if}
