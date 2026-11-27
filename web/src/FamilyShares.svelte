<script lang="ts">
  import { api, RequestError, type Session } from './identity';
  import type { Moment } from './journal';
  import type { Attachment } from './photos';
  import SharedMaterial from './SharedMaterial.svelte';
  import ShareThread from './ShareThread.svelte';
  import type { SharedMoment } from './shares';

  interface Review {
    guests: { id: string; display_name: string }[];
    shares: { id: string; guest_name: string; revision: number; state: string }[];
  }

  let { session, moment, onauthlost, oncomments }: {
    session: Session; moment: Moment; onauthlost: () => void; oncomments: () => void;
  } = $props();
  let review = $state<Review | null>(null);
  let files = $state<Attachment[]>([]);
  let selected = $state<string[]>([]);
  let text = $state(false);
  let guest = $state('');
  let busy = $state(false);
  let error = $state('');
  let code = $state('');
  let thread = $state('');
  let preview = $state<SharedMoment | null>(null);
  let reviewedGuest = $state('');
  let reviewedRevision = $state(0);
  let canShareText = $derived(!!moment.note && (moment.voice !== 'scribed' || moment.confirmed));

  async function load() {
    const [record, originals] = await Promise.all([
      api<Review>(`/moments/${moment.id}/shares`, 'GET', undefined, session.csrf_token),
      api<Attachment[]>(`/moments/${moment.id}/attachments`, 'GET', undefined, session.csrf_token),
    ]);
    review = record;
    files = originals.filter((file) => !file.source_url);
  }

  function failed(failure: unknown) {
    if (failure instanceof RequestError && [401, 403].includes(failure.status)) onauthlost();
    else error = 'The share could not be updated. Review the current moment and try again.';
  }

  function prepare(event: SubmitEvent) {
    event.preventDefault();
    const includeText = text && canShareText;
    const originals = files.filter(({ id }) => selected.includes(id));
    if ((!includeText && originals.length === 0) || !guest) return;
    error = '';
    code = '';
    reviewedGuest = guest;
    reviewedRevision = moment.revision;
    preview = { ...moment, id: crypto.randomUUID(), note: includeText ? moment.note : null, originals };
  }

  async function approve() {
    if (!preview || busy || reviewedRevision !== moment.revision) return;
    busy = true;
    try {
      const invitation = await api<{id: string; token: string}>(`/shares/${preview.id}`, 'PUT', {
        guest_id: reviewedGuest, moment_id: moment.id, revision: reviewedRevision,
        include_text: preview.note !== null, original_asset_ids: preview.originals.map(({ id }) => id),
      }, session.csrf_token);
      code = `${invitation.id}.${invitation.token}`;
      preview = null;
      await load();
    } catch (failure) {
      failed(failure);
    } finally {
      busy = false;
    }
  }

  async function withdraw(id: string) {
    if (busy) return;
    busy = true;
    try {
      await api(`/shares/${id}`, 'DELETE', undefined, session.csrf_token);
      if (code.startsWith(id)) code = '';
      if (thread === id) thread = '';
      await load();
      oncomments();
    } catch (failure) {
      failed(failure);
    } finally {
      busy = false;
    }
  }
</script>

<details ontoggle={(event) => { if (event.currentTarget.open) void load().catch(failed); }}>
  <summary>Share with a relative</summary>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if review}
    {#if review.guests.length === 0}
      <p>No relative accounts in this household yet.</p>
    {:else}
      <form onsubmit={prepare}>
        <label>Relative
          <select aria-label="Relative" bind:value={guest} required>
            <option value="">Choose a relative</option>
            {#each review.guests as choice (choice.id)}
              <option value={choice.id}>{choice.display_name}</option>
            {/each}
          </select>
        </label>
        <fieldset>
          <legend>What can they see?</legend>
          {#if canShareText}<label><input type="checkbox" bind:checked={text} /> Share these words</label>{/if}
          {#each files as file (file.id)}
            <label><input type="checkbox" bind:group={selected} value={file.id} />{file.label}</label>
          {/each}
        </fieldset>
        <button type="submit" disabled={!guest || (!text && selected.length === 0)}>Preview as relative</button>
      </form>
    {/if}
    {#if preview}
      <section aria-label="Guest preview">
        <h3>Guest preview for {review.guests.find((item) => item.id === reviewedGuest)?.display_name}</h3>
        <SharedMaterial item={preview} preview />
        {#if reviewedRevision !== moment.revision || (preview.note !== null && !canShareText)}
          <p>Review the current moment before sharing.</p>
        {:else if session.actor.role === 'guardian'}
          <button type="button" disabled={busy} onclick={approve}>Create invitation</button>
        {/if}
      </section>
    {/if}
    {#if code}
      <label>Code to give this relative <input readonly value={code} onclick={(event) => event.currentTarget.select()} /></label>
      <p>The relative signs in with their own account and enters this code within 24 hours.</p>
    {/if}
    {#if review.shares.length}
      <ul aria-label="Relative grants">
        {#each review.shares as share (share.id)}
          <li>
            <p>{share.guest_name} · {share.state} · Version {share.revision}</p>
            {#if share.state === 'Active'}
              <button type="button" class="quiet" onclick={() => { thread = thread === share.id ? '' : share.id; oncomments(); }}>
                {thread === share.id ? 'Close conversation' : 'Open conversation'}
              </button>
            {/if}
            {#if session.actor.role === 'guardian' && ['Active', 'Invited'].includes(share.state)}
              <button type="button" class="quiet" disabled={busy} onclick={() => withdraw(share.id)}>Withdraw share</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if thread}
      {#key thread}
        <ShareThread {session} id={thread} {onauthlost} onmissing={() => { thread = ''; void load().catch(failed); }} />
      {/key}
    {/if}
  {/if}
</details>

<style>
  details { margin: 1rem 0; }
  label { display: block; padding: .4rem 0; }
  fieldset, section { margin: .75rem 0; }
</style>
