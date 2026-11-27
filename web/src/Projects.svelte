<script lang="ts">
  import { onMount } from 'svelte';
  import { api, RequestError, type Session } from './identity';
  import { type Moment } from './journal';
  import { labelName, labelsFor, sourceName, type Catalog, type TimelineEntry } from './projects';
  import HouseholdLabels from './HouseholdLabels.svelte';
  import ProjectTimeline from './ProjectTimeline.svelte';
  import AttemptCard from './AttemptCard.svelte';
  import SkillEvidence from './SkillEvidence.svelte';

  let { session, entry, onauthlost }: {
    session: Session; entry: { kind: 'projects' | 'skills'; id: string } | null; onauthlost: () => void;
  } = $props();
  let catalog = $state<Catalog | null>(null);
  let moments = $state<Moment[]>([]);
  let project = $state('');
  let skill = $state('');
  let compare = $state<TimelineEntry[]>([]);
  let compareVersion = $state(0);
  let error = $state('');
  let title = $state('');
  let question = $state('');
  let criterion = $state('');
  let type = $state('');
  let domains = $state<string[]>([]);
  let id = crypto.randomUUID();
  let pending = $state(false);
  let selectedProject = $derived(catalog?.projects.find((item) => item.id === project));
  let selectedSkill = $derived(catalog?.labels.find((item) => item.kind === 'skill' && item.id === skill));
  let ordered = $derived([...compare].sort((a, b) => a.occurred_on.localeCompare(b.occurred_on)
    || a.recorded_at.localeCompare(b.recorded_at) || a.moment_id.localeCompare(b.moment_id)));

  function failed(failure: unknown) {
    if (failure instanceof RequestError && [401, 403].includes(failure.status)) onauthlost();
    else if (failure instanceof RequestError && failure.status === 409) {
      error = 'This record changed or already exists. Reload Projects to review the saved version.';
    } else if (failure instanceof RequestError && failure.status === 422) {
      error = 'Check the selected items and details before saving.';
    } else {
      error = 'Projects or evidence could not be updated. Try again.';
    }
  }

  async function loadCatalog() {
    catalog = await api<Catalog>('/projects', 'GET', undefined, session.csrf_token);
  }

  async function load() {
    await Promise.all([
      loadCatalog(),
      api<Moment[]>('/moments', 'GET', undefined, session.csrf_token).then((items) => { moments = items; }),
    ]);
  }

  onMount(() => {
    if (entry?.kind === 'projects') project = entry.id;
    if (entry?.kind === 'skills') skill = entry.id;
    void load().catch(failed);
  });

  async function addProject(event: SubmitEvent) {
    event.preventDefault();
    if (pending) return;
    pending = true;
    error = '';
    try {
      await api(`/projects/${id}`, 'PUT', {
        title, question, success_criterion: criterion, type_id: type || null, domain_ids: domains,
      }, session.csrf_token);
      await loadCatalog();
      project = id;
      skill = '';
      id = crypto.randomUUID();
      title = ''; question = ''; criterion = ''; type = ''; domains = [];
    } catch (failure) {
      failed(failure);
    } finally {
      pending = false;
    }
  }

  function toggle(entry: TimelineEntry) {
    if (compare.some(({ moment_id }) => moment_id === entry.moment_id)) {
      compare = compare.filter(({ moment_id }) => moment_id !== entry.moment_id);
    } else if (compare.length < 2) {
      compare = [...compare, entry];
    }
  }

  function refreshCompare(loads: TimelineEntry[]) {
    compare = compare.map((old) => loads.find(({ withdrawn, moment_id }) => !withdrawn && moment_id === old.moment_id) ?? old);
    compareVersion++;
  }
</script>

{#if error}<p role="alert">{error} <button type="button" class="quiet" onclick={() => error = ''}>Dismiss</button></p>{/if}
{#if catalog}
  <details>
    <summary>Add a project</summary>
    <form onsubmit={addProject}>
      <label>Project title <input bind:value={title} required maxlength="200" /></label>
      <label>Question <input bind:value={question} /></label>
      <label>What success looks like <input bind:value={criterion} /></label>
      {#if labelsFor(catalog.labels, 'project_type').length}
        <label>Project type
          <select aria-label="Project type" bind:value={type}>
            <option value="">None</option>
            {#each labelsFor(catalog.labels, 'project_type') as item (item.id)}<option value={item.id}>{item.name}</option>{/each}
          </select>
        </label>
      {/if}
      {#if labelsFor(catalog.labels, 'domain').length}
        <fieldset>
          <legend>Domains (optional)</legend>
          {#each labelsFor(catalog.labels, 'domain') as item (item.id)}
            <label><input type="checkbox" bind:group={domains} value={item.id} />{item.name}</label>
          {/each}
        </fieldset>
      {/if}
      <button type="submit" disabled={pending}>Save project</button>
    </form>
  </details>
  {#if session.actor.role === 'guardian'}
    <HouseholdLabels {session} labels={catalog.labels} onchanged={loadCatalog} onfailure={failed} />
  {/if}
  {#if catalog.projects.length}
    <label>Choose a project
      <select aria-label="Choose a project" bind:value={project} onchange={() => skill = ''}>
        <option value="">Choose a project</option>
        {#each catalog.projects as item (item.id)}<option value={item.id}>{item.title}</option>{/each}
      </select>
    </label>
  {:else}
    <p>No projects yet.</p>
  {/if}
  <label>Choose a skill
    <select aria-label="Choose a skill" bind:value={skill} onchange={() => project = ''}>
      <option value="">Choose a skill</option>
      {#each catalog.labels.filter((item) => item.kind === 'skill') as item (item.id)}
        <option value={item.id}>{labelName(catalog.labels, item.id)}</option>
      {/each}
    </select>
  </label>
  {#if (project && !selectedProject) || (skill && !selectedSkill)}<p>This item is unavailable.</p>{/if}
  {#if selectedProject}
    {#key selectedProject.id}
      <ProjectTimeline project={selectedProject} {session} labels={catalog.labels} {moments} {compare}
        oncompare={toggle} onchanged={refreshCompare} onfailure={failed} />
    {/key}
  {/if}
  {#if selectedSkill}
    {#key selectedSkill.id}
      <SkillEvidence label={selectedSkill} projects={catalog.projects} {session} {moments} onfailure={failed} />
    {/key}
  {/if}
  {#if ordered.length}
    <section aria-label="Attempt comparison">
      <h2>Compare attempts</h2>
      {#if ordered.length === 1}<p>Choose one more attempt.</p>{/if}
      <div class="comparison">
        {#key compareVersion}
          {#each ordered as entry (entry.moment_id)}
            <div>
              <AttemptCard {entry} {session} labels={catalog.labels} onfailure={failed} />
              <button type="button" class="quiet" onclick={() => toggle(entry)}>Remove {sourceName(entry)} from comparison</button>
            </div>
          {/each}
        {/key}
      </div>
    </section>
  {/if}
{:else if !error}
  <p role="status">Loading projects…</p>
{/if}

<style>
  label { display: block; margin-block: .5rem; }
  details, section { margin-block: 1rem; }
  summary { cursor: pointer; }
  .comparison { display: grid; gap: 1rem; grid-template-columns: repeat(auto-fit, minmax(min(100%, 17rem), 1fr)); }
</style>
