<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, RequestError, type Session } from './identity';
  import { attribution, eventDate, readableDate, type Comment, type Moment, type MomentDraft } from './journal';

  const { session, moment, draft, onupdate, onauthlost }: {
    session: Session;
    moment: Moment;
    draft: MomentDraft;
    onupdate: (moment: Moment) => void;
    onauthlost: () => void;
  } = $props();

  let comments = $state<Comment[] | null>(null);
  let history = $state<Moment[] | null>(null);
  let error = $state('');
  let status = $state('');
  let saving = $state(false);
  let conflict = $state<Moment | null>(null);
  let editInput = $state<HTMLTextAreaElement>();
  let alive = true;

  let endpoint = $derived(`/moments/${moment.id}`);
  let isWriter = $derived((moment.voice === 'guardian' ? moment.recorder_id : moment.learner_id) === session.actor.id);
  let canConfirm = $derived(moment.voice === 'scribed' && moment.learner_id === session.actor.id);

  onMount(() => {
    void loadComments();
    return () => { alive = false; };
  });

  function isSessionError(failure: unknown) {
    if (failure instanceof RequestError && (failure.status === 401 || failure.status === 403)) {
      onauthlost();
      return true;
    }
    return false;
  }

  async function loadComments() {
    try {
      const loaded = await api<Comment[]>(`${endpoint}/comments`, 'GET', undefined, session.csrf_token);
      if (alive) comments = loaded;
    } catch (failure) {
      if (alive && !isSessionError(failure)) error = 'Comments could not load.';
    }
  }

  async function loadHistory() {
    error = '';
    try {
      const loaded = await api<Moment[]>(`${endpoint}/revisions`, 'GET', undefined, session.csrf_token);
      if (alive) history = loaded;
    } catch (failure) {
      if (alive && !isSessionError(failure)) error = 'Earlier versions could not load.';
    }
  }

  async function startEdit(source: Moment = moment) {
    if (draft.edit && source === moment) return;
    draft.edit = {
      revision: moment.revision,
      note: source.note ?? '',
      activityDate: source.activity_date ?? '',
    };
    conflict = null;
    error = '';
    status = '';
    await tick();
    editInput?.focus();
  }

  function beginSave() {
    error = '';
    status = '';
    saving = true;
  }

  function savedVersion(failure: unknown): Moment | null {
    if (!(failure instanceof RequestError) || failure.status !== 409) return null;
    const conflict = failure.body as { saved?: Moment } | null;
    return conflict?.saved ?? null;
  }

  async function saveEdit(event: SubmitEvent) {
    event.preventDefault();
    if (!draft.edit || saving) return;
    const { revision, note, activityDate } = draft.edit;
    beginSave();
    try {
      const updated = await api<Moment>(endpoint, 'PATCH', {
        expected_revision: revision, note, activity_date: activityDate || null,
      }, session.csrf_token);
      if (!alive) return;
      onupdate(updated);
      draft.edit = undefined;
      conflict = null;
      history = null;
      status = 'Changes saved.';
    } catch (failure) {
      if (!alive || isSessionError(failure)) return;
      const newer = savedVersion(failure);
      if (newer) {
        conflict = newer;
        onupdate(newer);
        error = 'This moment changed in another tab. Review the saved words before retrying.';
      } else {
        error = 'The edit could not be saved. Check the words and activity date.';
      }
    } finally {
      saving = false;
    }
  }

  async function confirm(value: boolean) {
    if (saving) return;
    beginSave();
    try {
      const updated = await api<Moment>(`${endpoint}/confirmation`, value ? 'POST' : 'DELETE',
        { expected_revision: moment.revision }, session.csrf_token);
      if (!alive) return;
      onupdate(updated);
      status = value ? 'Words confirmed.' : 'Confirmation withdrawn.';
    } catch (failure) {
      if (!alive || isSessionError(failure)) return;
      const newer = savedVersion(failure);
      if (newer) onupdate(newer);
      error = newer ? 'These words changed. Read the current version first.' : 'The confirmation could not be updated.';
    } finally {
      saving = false;
    }
  }

  async function postComment(event: SubmitEvent) {
    event.preventDefault();
    if (saving) return;
    beginSave();
    try {
      const comment = await api<Comment>(`${endpoint}/comments`, 'POST', {
        body: draft.comment, kind: draft.kind, submission_key: draft.submission,
      }, session.csrf_token);
      if (!alive) return;
      comments = [...(comments ?? []).filter((current) => current.id !== comment.id), comment];
      draft.comment = '';
      draft.submission = crypto.randomUUID();
      status = 'Comment posted.';
    } catch (failure) {
      if (!alive || isSessionError(failure)) return;
      error = 'The comment could not be posted. Your draft is here to retry.';
    } finally {
      saving = false;
    }
  }

  async function toggleComment(comment: Comment) {
    if (saving) return;
    beginSave();
    try {
      const path = `${endpoint}/comments/${comment.id}`;
      if (comment.withdrawn) {
        await api<Comment>(`${path}/restore`, 'POST', undefined, session.csrf_token);
      } else {
        await api<void>(path, 'DELETE', undefined, session.csrf_token);
      }
      if (!alive) return;
      comments = comments?.map((item) => item.id === comment.id ? { ...item, withdrawn: !item.withdrawn } : item) ?? [];
      status = comment.withdrawn ? 'Comment restored.' : 'Comment withdrawn.';
    } catch (failure) {
      if (!alive || isSessionError(failure)) return;
      error = 'The comment could not be updated.';
    } finally {
      saving = false;
    }
  }
</script>

<p class="hint">{attribution(moment)} · {eventDate(moment)}</p>
{#if moment.note}<p class="prose">{moment.note}</p>{/if}
{#if moment.asset_ids.length}<p>{moment.asset_ids.length} {moment.asset_ids.length === 1 ? 'attachment' : 'attachments'}</p>{/if}
{#if moment.voice === 'scribed'}
  <p>{moment.confirmed ? 'Confirmed by the learner.' : 'Awaiting learner confirmation.'}</p>
{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if status}<p role="status">{status}</p>{/if}
<div class="actions">
  {#if isWriter && !draft.edit}
    <button type="button" class="quiet" disabled={saving} onclick={() => startEdit()}>Edit words</button>
  {/if}
  {#if canConfirm && !draft.edit}
    <button type="button" class="quiet" disabled={saving} onclick={() => confirm(!moment.confirmed)}>
      {moment.confirmed ? 'Withdraw confirmation' : 'Confirm these words'}
    </button>
  {/if}
  {#if moment.revision > 1 && history === null}
    <button type="button" class="quiet" onclick={loadHistory}>Earlier versions</button>
  {/if}
</div>

{#if draft.edit}
  <form onsubmit={saveEdit}>
    <label for="edit-words">Your words</label>
    <textarea id="edit-words" rows="5" bind:this={editInput} bind:value={draft.edit.note} disabled={saving} autocomplete="off"></textarea>
    <label for="edit-date">Activity date (optional)</label>
    <input id="edit-date" type="date" bind:value={draft.edit.activityDate} disabled={saving} />
    {#if conflict}
      <p>Saved version {conflict.revision}</p>
      {#if conflict.note}<p class="prose">{conflict.note}</p>{/if}
      <button type="button" class="quiet" onclick={() => {
        if (draft.edit && conflict) draft.edit.revision = conflict.revision;
        conflict = null; error = '';
      }}>Keep my draft and use the latest revision</button>
    {/if}
    <div class="actions">
      <button type="submit" disabled={saving || conflict !== null}>Save changes</button>
      <button type="button" class="quiet" disabled={saving} onclick={() => {
        draft.edit = undefined; conflict = null; error = '';
      }}>Discard edit</button>
    </div>
  </form>
{/if}

{#if history}
  <h2>Earlier versions</h2>
  {#each history.filter((version) => version.revision < moment.revision) as version (version.revision)}
    <section class="entry">
      <h3>Version {version.revision}</h3>
      <p class="hint">{version.editor_name} · {readableDate(version.updated_at)}</p>
      {#if version.note}<p class="prose">{version.note}</p>{/if}
      {#if isWriter && !draft.edit}
        <button type="button" class="quiet" disabled={saving} onclick={() => startEdit(version)}>
          Restore version {version.revision}
        </button>
      {/if}
    </section>
  {/each}
{/if}

<h2>Comments</h2>
{#if comments === null}<button type="button" class="quiet" onclick={loadComments}>Load comments</button>
{:else if comments.length === 0}<p>No comments yet.</p>
{:else}
  <ul>
    {#each comments as comment (comment.id)}
      <li class="entry">
        <p class="hint">{comment.kind === 'suggestion' ? 'Suggestion' : 'Comment'} · {comment.author_name} · {readableDate(comment.created_at)}</p>
        <p class="prose">{comment.body}</p>
        {#if comment.author_id === session.actor.id}
          <button type="button" class="quiet" disabled={saving} onclick={() => toggleComment(comment)}>
            {comment.withdrawn ? 'Restore comment' : 'Withdraw comment'}
          </button>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<form onsubmit={postComment}>
  <label for="comment-kind">Reply type</label>
  <select id="comment-kind" bind:value={draft.kind} disabled={saving}>
    <option value="comment">Comment</option>
    <option value="suggestion">Suggestion</option>
  </select>
  <label for="comment">Your comment</label>
  <textarea id="comment" rows="3" bind:value={draft.comment} disabled={saving} autocomplete="off"></textarea>
  <div class="actions"><button type="submit" disabled={saving || comments === null}>Post comment</button></div>
</form>

<style>
  h2 { font-size: 1.25rem; margin-top: 1.5rem; }
  h3 { font-size: 1.1rem; }
  label { display: block; font-weight: 650; margin: .5rem 0; }
  form { margin: 1rem 0; }
  select, input[type="date"] { max-width: 16rem; }
  .actions { display: flex; flex-wrap: wrap; gap: .5rem; margin: 1rem 0; }
  .prose { white-space: pre-wrap; overflow-wrap: anywhere; }
  .hint { color: #483f34; }
  .error { color: #8b2620; }
  ul { padding: 0; list-style: none; }
  .entry { border-top: 1px solid #b8bbae; padding: .4rem 0; overflow-wrap: anywhere; }
</style>
