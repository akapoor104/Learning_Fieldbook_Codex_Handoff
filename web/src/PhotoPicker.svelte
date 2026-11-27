<script lang="ts">
  import { api, RequestError, type Session } from './identity';
  import { fileTypes, inbox, matchPath, photoTypes, pickerLimit, uploadFile } from './photos';

  const { session, learner, moment, onauthlost, onsaved }: {
    session: Session;
    learner: string;
    moment?: string;
    onauthlost: () => void;
    onsaved: () => void;
  } = $props();

  let file = $state<File | null>(null);
  let photoInput = $state<HTMLInputElement>();
  let fileInput = $state<HTMLInputElement>();
  let date = $state('');
  let submission = '';
  let asset = $state<string | null>(null);
  let sending = $state(false);
  let status = $state('');
  let error = $state('');
  let queued = $state(false);

  function choose(event: Event & { currentTarget: HTMLInputElement }) {
    const next = event.currentTarget.files?.[0];
    event.currentTarget.value = '';
    if (!next) return;
    error = '';
    status = '';
    if (next.size === 0 || next.size > pickerLimit) {
      error = 'Choose a file between 1 byte and 64 MiB.';
      return;
    }
    file = next;
    submission = crypto.randomUUID();
    asset = null;
    queued = false;
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!file || sending) return;
    sending = true;
    error = '';
    status = 'Saving file…';
    try {
      asset ??= await uploadFile(file, submission, session.csrf_token, (bytes) => {
        status = `Uploading ${Math.floor(bytes / file!.size * 100)}%`;
      });
      if (!queued) {
        await api<void>(`${inbox}/${learner}/${asset}`, 'PUT', { activity_date: date || null }, session.csrf_token);
        queued = true;
      }
      if (moment) await api<void>(matchPath(asset, moment), 'POST', undefined, session.csrf_token);
      status = moment ? 'File attached.' : 'File saved to Family photos to match.';
      file = null;
      asset = null;
      date = '';
      queued = false;
      onsaved();
    } catch (failure) {
      status = '';
      if (failure instanceof RequestError && [401, 403].includes(failure.status)) {
        onauthlost();
      } else if (queued) {
        error = 'The file is in Family photos to match. Try attaching it again.';
        onsaved();
      } else if (failure instanceof RequestError && failure.status === 422) {
        error = 'Check the activity date or choose a supported file.';
      } else {
        error = 'The file could not be saved. Use Save to retry.';
      }
    } finally {
      sending = false;
    }
  }
</script>

<form onsubmit={save} aria-label={moment ? 'Attach a family file' : 'Save a family file for matching'}>
  <div class="actions">
    <button type="button" class="quiet" disabled={sending} onclick={() => photoInput?.click()}>Choose photos</button>
    <button type="button" class="quiet" disabled={sending} onclick={() => fileInput?.click()}>Choose a file</button>
  </div>
  <input type="file" hidden aria-label="Photo picker" accept={photoTypes} bind:this={photoInput} onchange={choose} />
  <input type="file" hidden aria-label="File picker" accept={fileTypes} bind:this={fileInput} onchange={choose} />
  {#if file}
    <p class="filename">{file.name}</p>
    {#if !moment}
      <label>Photo activity date (optional)
        <input type="date" bind:value={date} disabled={sending || queued} />
      </label>
    {/if}
    <button type="submit" disabled={sending}>{moment ? 'Save and attach' : 'Save for matching'}</button>
  {/if}
  {#if status}<p role="status">{status}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</form>

<style>
  .actions { display: flex; flex-wrap: wrap; gap: .5rem; }
  .filename { overflow-wrap: anywhere; }
  label { display: block; margin: .75rem 0; }
  input[type="date"] { display: block; max-width: 16rem; }
  .error { color: #8b2620; }
</style>
