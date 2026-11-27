<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Session } from './identity';
  import type { Moment } from './journal';
  import { sourceName, sourceVoice, type Label, type Project, type Source } from './projects';

  interface Evidence extends Source {
    learner_id: string;
    reason: string;
    withdrawn: boolean;
    independence: 'on_my_own' | 'with_help' | null;
    project_ids: string[];
  }

  interface Transfer {
    id: string;
    learner_id: string;
    learner_name: string;
    from_moment_id: string;
    from_project_id: string;
    to_moment_id: string;
    to_project_id: string;
    reason: string;
    withdrawn: boolean;
    sources_active: boolean;
  }

  interface Skill { evidence: Evidence[]; transfers: Transfer[] }

  let { label, projects, session, moments, onfailure }: {
    label: Label; projects: Project[]; session: Session; moments: Moment[]; onfailure: (error: unknown) => void;
  } = $props();

  let skill = $state<Skill | null>(null);
  let source = $state('');
  let reason = $state('');
  let independence = $state('');
  let from = $state('');
  let to = $state('');
  let transferReason = $state('');
  let transferId = crypto.randomUUID();
  let pending = $state(false);
  let notice = $state('');
  let owned = $derived(skill?.evidence.filter((item) => !item.withdrawn && item.learner_id === session.actor.id) ?? []);
  let options = $derived(owned.flatMap((item) => item.project_ids.map((project) => ({
    moment_id: item.moment_id, project_id: project, value: `${item.moment_id}/${project}`,
    name: `${projects.find(({ id }) => id === project)?.title ?? 'Project'} · ${sourceName(item)}`,
  }))));
  let destinations = $derived(options.filter((item) => {
    const start = options.find(({ value }) => value === from);
    return !start || (item.moment_id !== start.moment_id && item.project_id !== start.project_id);
  }));

  async function load() {
    skill = await api<Skill>(`/skills/${label.id}`, 'GET', undefined, session.csrf_token);
  }

  onMount(() => { void load().catch(onfailure); });

  async function mutate(route: string, method: string, body?: object) {
    if (pending) return false;
    pending = true;
    notice = '';
    try {
      await api(route, method, body, session.csrf_token);
      await load();
      return true;
    } catch (failure) {
      onfailure(failure);
      return false;
    } finally {
      pending = false;
    }
  }

  async function addEvidence(event: SubmitEvent) {
    event.preventDefault();
    if (await mutate(`/moments/${source}/skills/${label.id}`, 'PUT', { reason, independence: independence || null })) {
      source = ''; reason = ''; independence = '';
      notice = 'Skill evidence linked.';
    }
  }

  async function addTransfer(event: SubmitEvent) {
    event.preventDefault();
    const start = options.find(({ value }) => value === from);
    const end = destinations.find(({ value }) => value === to);
    if (!start || !end) return;
    if (await mutate(`/skill-transfers/${transferId}`, 'PUT', {
      label_id: label.id, from_project_id: start.project_id, from_moment_id: start.moment_id,
      to_project_id: end.project_id, to_moment_id: end.moment_id, reason: transferReason,
    })) {
      transferId = crypto.randomUUID();
      from = ''; to = ''; transferReason = '';
      notice = 'Skill transfer recorded.';
    }
  }

  function evidencePath(id: string) {
    return `/moments/${id}/skills/${label.id}`;
  }
</script>

<section aria-label="Skill evidence">
  <h2>{label.name}{label.retired ? ' (retired)' : ''}</h2>
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if skill}
    {#if skill.evidence.every((item) => item.withdrawn)}<p>No demonstrated evidence yet.</p>{/if}
    <ol aria-label="Evidence examples">
      {#each skill.evidence.filter((item) => !item.withdrawn) as item (item.moment_id)}
        <li>
          <p>{item.activity_date ?? 'Activity date not recorded'} · {sourceVoice(item)}</p>
          <p><a href={`/?moment=${item.moment_id}`}>{sourceName(item)}</a></p>
          <p><strong>{item.learner_name}’s reason</strong> <span>{item.reason}</span></p>
          {#if item.independence}<p>{item.independence === 'with_help' ? 'With help' : 'On my own'}</p>{/if}
          {#if item.learner_id === session.actor.id}
            <button type="button" class="quiet" disabled={pending} onclick={() => mutate(evidencePath(item.moment_id), 'DELETE')}>
              Remove skill connection
            </button>
          {/if}
        </li>
      {/each}
    </ol>
    {#if skill.evidence.some((item) => item.withdrawn)}
      <details>
        <summary>Removed skill connections</summary>
        <ul>
          {#each skill.evidence.filter((item) => item.withdrawn) as item (item.moment_id)}
            <li>{sourceName(item)}
              <button type="button" class="quiet" disabled={pending} onclick={() => mutate(`${evidencePath(item.moment_id)}/restore`, 'POST')}>
                Restore skill connection
              </button>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
    {#if !label.retired && session.actor.role === 'learner'}
      {@const choices = moments.filter((item) => item.learner_id === session.actor.id && !skill?.evidence.some((entry) => entry.moment_id === item.id))}
      {#if choices.length}
        <details>
          <summary>Link an evidence moment</summary>
          <form onsubmit={addEvidence}>
            <label>Evidence moment
              <select aria-label="Evidence moment" bind:value={source} required>
                <option value="">Choose a moment</option>
                {#each choices as entry (entry.id)}<option value={entry.id}>{entry.note ?? 'Moment with attachments'}</option>{/each}
              </select>
            </label>
            <label>How does this show the skill? <textarea bind:value={reason} rows="2" required></textarea></label>
            <label>How I worked
              <select aria-label="How I worked" bind:value={independence}>
                <option value="">No description</option>
                <option value="with_help">With help</option>
                <option value="on_my_own">On my own</option>
              </select>
            </label>
            <button type="submit" disabled={pending}>Save skill reason</button>
          </form>
        </details>
      {/if}
      {#if options.some((first) => options.some((second) => first.moment_id !== second.moment_id && first.project_id !== second.project_id))}
        <details>
          <summary>Record a transfer</summary>
          <form onsubmit={addTransfer}>
            <label>Earlier project and moment
              <select aria-label="Earlier project and moment" bind:value={from} required onchange={() => to = ''}>
                <option value="">Choose an earlier example</option>
                {#each options as item (item.value)}<option value={item.value}>{item.name}</option>{/each}
              </select>
            </label>
            <label>New project and moment
              <select aria-label="New project and moment" bind:value={to} required disabled={!from}>
                <option value="">Choose the new example</option>
                {#each destinations as item (item.value)}<option value={item.value}>{item.name}</option>{/each}
              </select>
            </label>
            <label>Why did this skill carry over? <textarea bind:value={transferReason} rows="2" required></textarea></label>
            <button type="submit" disabled={pending}>Save transfer</button>
          </form>
        </details>
      {/if}
    {/if}
    {#if skill.transfers.length}
      <h3>Skill transfers</h3>
      <ul>
        {#each skill.transfers as item (item.id)}
          <li>
            <p>{item.learner_name} · {item.reason}</p>
            <p><a href={`/?moment=${item.from_moment_id}`}>{projects.find(({ id }) => id === item.from_project_id)?.title} source</a>
              and <a href={`/?moment=${item.to_moment_id}`}>{projects.find(({ id }) => id === item.to_project_id)?.title} source</a></p>
            {#if !item.sources_active}<p>A supporting connection was removed.</p>{/if}
            {#if item.learner_id === session.actor.id}
              {#if item.withdrawn}<p>Transfer withdrawn.</p>{/if}
              <button type="button" class="quiet" disabled={pending || (item.withdrawn && !item.sources_active)}
                onclick={() => mutate(`/skill-transfers/${item.id}${item.withdrawn ? '/restore' : ''}`, item.withdrawn ? 'POST' : 'DELETE')}>
                {item.withdrawn ? 'Restore transfer' : 'Withdraw transfer'}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {:else}
    <p role="status">Loading skill evidence…</p>
  {/if}
</section>

<style>
  label { display: block; margin-block: .5rem; }
  details, li { margin-block: .75rem; }
  li { overflow-wrap: anywhere; }
  li p { margin-block: .4rem; }
</style>
