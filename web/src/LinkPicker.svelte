<script lang="ts">
  import { tick } from 'svelte';
  import { api, RequestError, type Session } from './identity';
  import { attachmentPath } from './photos';

  const { session, moment, onsaved, onauthlost }: {
    session: Session;
    moment: string;
    onsaved: () => void;
    onauthlost: () => void;
  } = $props();

  let open = $state(false);
  let label = $state('');
  let source = $state('');
  let saving = $state(false);
  let error = $state('');
  let status = $state('');
  let field = $state<HTMLInputElement>();
  let trigger = $state<HTMLButtonElement>();
  let submission = crypto.randomUUID();

  async function changeOpen(value: boolean) {
    open = value;
    status = '';
    await tick();
    if (value) field?.focus();
    else trigger?.focus();
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (saving) return;
    saving = true;
    error = '';
    try {
      const { id } = await api<{ id: string }>('/assets/links', 'POST', {
        submission_key: submission, label, source_url: source,
      }, session.csrf_token);
      await api<void>(attachmentPath(moment, id), 'PUT', undefined, session.csrf_token);
      label = '';
      source = '';
      submission = crypto.randomUUID();
      await changeOpen(false);
      status = 'Link saved.';
      onsaved();
    } catch (failure) {
      const code = failure instanceof RequestError ? failure.status : 0;
      if (code === 401 || code === 403) {
        onauthlost();
      } else if (code === 422) {
        const details = (failure as RequestError).body as { error?: string } | null;
        error = details?.error === 'too_many_assets'
          ? 'This moment already has 20 attachments.'
          : 'Enter a label and an HTTPS link.';
      } else {
        error = 'The link could not be saved. Your draft is here to retry.';
      }
    } finally {
      saving = false;
    }
  }
</script>

{#if open}
  <form onsubmit={save} aria-label="Paste a Drive link">
    <label>Link label
      <input bind:this={field} bind:value={label} oninput={() => submission = crypto.randomUUID()}
        required maxlength="255" disabled={saving} />
    </label>
    <label>HTTPS link
      <input type="url" bind:value={source} oninput={() => submission = crypto.randomUUID()}
        required maxlength="4096" disabled={saving} />
    </label>
    <div class="actions">
      <button type="submit" disabled={saving}>Save link</button>
      <button type="button" class="quiet" disabled={saving} onclick={() => changeOpen(false)}>Cancel</button>
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
{:else}
  <button type="button" class="quiet" bind:this={trigger} onclick={() => changeOpen(true)}>Paste a Drive link</button>
  {#if status}<p role="status">{status}</p>{/if}
{/if}

<style>
  form { margin: .75rem 0; }
  label, input { display: block; }
  label { margin: .75rem 0; }
  .actions { display: flex; gap: .5rem; }
  .error { color: #8b2620; }
</style>
