<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Session } from './identity';
  import { type Moment } from './journal';
  import { labelName, sourceName, sourceVoice, type Label, type Project, type TimelineEntry } from './projects';
  import AttemptEditor from './AttemptEditor.svelte';

  let { project, session, labels, moments, compare, oncompare, onchanged, onfailure }: {
    project: Project; session: Session; labels: Label[]; moments: Moment[]; compare: TimelineEntry[];
    oncompare: (entry: TimelineEntry) => void; onchanged: (entries: TimelineEntry[]) => void;
    onfailure: (error: unknown) => void;
  } = $props();

  let entries = $state<TimelineEntry[] | null>(null);
  let source = $state('');
  let editing = $state('');
  let pending = $state(false);
  let endpoint = $derived(`/projects/${project.id}`);
  let choices = $derived(moments.filter((moment) => !entries?.some((entry) => entry.moment_id === moment.id)));

  async function load() {
    entries = await api<TimelineEntry[]>(endpoint, 'GET', undefined, session.csrf_token);
    onchanged(entries);
  }

  onMount(() => { void load().catch(onfailure); });

  function isWriter(entry: TimelineEntry) {
    const moment = moments.find(({ id }) => id === entry.moment_id);
    return (moment?.voice === 'guardian' ? moment.recorder_id : moment?.learner_id) === session.actor.id;
  }

  async function change(id: string, link: boolean) {
    if (!id || pending) return;
    pending = true;
    try {
      await api(`${endpoint}/moments/${id}`, link ? 'PUT' : 'DELETE', undefined, session.csrf_token);
      if (source === id) source = '';
      if (!link && editing === id) editing = '';
      await load();
    } catch (failure) {
      onfailure(failure);
    } finally {
      pending = false;
    }
  }
</script>

<section aria-label="Project timeline">
  <h2>{project.title}</h2>
  {#if project.type_id || project.domain_ids.length}
    <p>{[project.type_id, ...project.domain_ids].map((id) => labelName(labels, id)).filter(Boolean).join(' · ')}</p>
  {/if}
  {#if project.question}<p><strong>Question</strong> {project.question}</p>{/if}
  {#if project.success_criterion}<p><strong>What success looks like</strong> {project.success_criterion}</p>{/if}
  <h3>Activity timeline</h3>
  {#if entries}
    {#if choices.length}
      <form onsubmit={(event) => { event.preventDefault(); void change(source, true); }}>
        <label>Saved family moment
          <select aria-label="Saved family moment" bind:value={source} required>
            <option value="">Choose a moment</option>
            {#each choices as moment (moment.id)}
              <option value={moment.id}>{moment.note ?? 'Moment with attachments'}{moment.activity_date ? ` · ${moment.activity_date}` : ''}</option>
            {/each}
          </select>
        </label>
        <button type="submit" disabled={!source || pending}>Link moment to project</button>
      </form>
    {/if}
    {#if entries.every((entry) => entry.withdrawn)}<p>No moments linked yet.</p>{/if}
    <ol aria-label="Activity order">
      {#each entries.filter((entry) => !entry.withdrawn) as entry (entry.moment_id)}
        <li>
          <p>{entry.activity_date ?? 'Activity date not recorded'} · {sourceVoice(entry)}</p>
          <p><a href={`/?moment=${entry.moment_id}`}>{sourceName(entry)}</a></p>
          <div class="actions">
            {#if entry.primary_project_id}
              <button type="button" class="quiet" aria-pressed={compare.some(({ moment_id }) => moment_id === entry.moment_id)}
                disabled={compare.length >= 2 && !compare.some(({ moment_id }) => moment_id === entry.moment_id)}
                onclick={() => oncompare(entry)}>Compare this attempt</button>
            {/if}
            {#if (!entry.primary_project_id || entry.primary_project_id === project.id) && isWriter(entry)}
              <button type="button" class="quiet" aria-expanded={editing === entry.moment_id}
                onclick={() => editing = editing === entry.moment_id ? '' : entry.moment_id}>
                {editing === entry.moment_id ? 'Close attempt editor' : entry.primary_project_id ? 'Edit attempt' : 'Record as attempt'}
              </button>
            {/if}
            {#if entry.primary_project_id !== project.id}
              <button type="button" class="quiet" disabled={pending} onclick={() => change(entry.moment_id, false)}>Remove project connection</button>
            {/if}
          </div>
          {#if editing === entry.moment_id}
            <AttemptEditor source={entry.moment_id} project={project.id} {entries} {session} {labels} onchanged={load} {onfailure} />
          {/if}
        </li>
      {/each}
    </ol>
    {#if entries.some((entry) => entry.withdrawn)}
      <details>
        <summary>Removed project connections</summary>
        <ul>
          {#each entries.filter((entry) => entry.withdrawn) as entry (entry.moment_id)}
            <li>{sourceName(entry)} <button type="button" class="quiet" disabled={pending} onclick={() => change(entry.moment_id, true)}>Restore project connection</button></li>
          {/each}
        </ul>
      </details>
    {/if}
  {:else}
    <p role="status">Loading timeline…</p>
  {/if}
</section>

<style>
  label { display: block; margin-block: .5rem; }
  li { margin-block: .8rem; overflow-wrap: anywhere; }
  li p { margin-block: .4rem; }
  .actions { display: flex; flex-wrap: wrap; gap: .4rem; }
</style>
