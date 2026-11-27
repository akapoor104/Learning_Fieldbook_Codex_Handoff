<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Session } from './identity';
  import { emptyAttempt, labelName, labelsFor, sourceName, type Attempt, type AttemptDetails, type Label, type TimelineEntry } from './projects';

  let { source, project, entries, session, labels, onchanged, onfailure }: {
    source: string; project: string; entries: TimelineEntry[]; session: Session; labels: Label[];
    onchanged: () => Promise<void>; onfailure: (error: unknown) => void;
  } = $props();

  let versions = $state<Attempt[] | null>(null);
  let draft = $state<AttemptDetails>(emptyAttempt());
  let pending = $state(false);
  let notice = $state('');
  let endpoint = $derived(`/moments/${source}/attempt`);
  let current = $derived(versions?.[0]);
  let roleChoices = $derived(labelsFor(labels, 'role', current?.details.role_ids));

  async function load() {
    versions = await api<Attempt[]>(endpoint, 'GET', undefined, session.csrf_token);
    const details = versions[0]?.details;
    draft = details ? { ...details, role_ids: [...details.role_ids] } : emptyAttempt();
  }

  onMount(() => { void load().catch(onfailure); });

  async function save(event?: SubmitEvent, restore?: number) {
    event?.preventDefault();
    if (pending || !versions) return;
    pending = true;
    try {
      const expected_revision = current?.revision ?? 0;
      if (restore === undefined) {
        await api(endpoint, 'PUT', {
          expected_revision, project_id: project, details: {
            ...draft, event_type_id: draft.event_type_id || null, prior_attempt_id: draft.prior_attempt_id || null,
          },
        }, session.csrf_token);
      } else {
        await api(`${endpoint}/restore`, 'POST', { expected_revision, restore_revision: restore }, session.csrf_token);
      }
      await load();
      await onchanged();
      notice = restore === undefined ? 'Attempt saved.' : 'Attempt version restored.';
    } catch (failure) {
      onfailure(failure);
    } finally {
      pending = false;
    }
  }
</script>

{#if versions}
  <form onsubmit={(event) => save(event)}>
    <label>Event or activity <input bind:value={draft.event} /></label>
    {#if labelsFor(labels, 'event_type', current?.details.event_type_id ? [current.details.event_type_id] : []).length}
      <label>Event type
        <select aria-label="Event type" bind:value={draft.event_type_id}>
          <option value={null}>None</option>
          {#each labelsFor(labels, 'event_type', current?.details.event_type_id ? [current.details.event_type_id] : []) as role (role.id)}
            <option value={role.id}>{labelName(labels, role.id)}</option>
          {/each}
        </select>
      </label>
    {/if}
    {#if roleChoices.length}
      <fieldset>
        <legend>Roles at this event</legend>
        {#each roleChoices as role (role.id)}
          <label><input type="checkbox" bind:group={draft.role_ids} value={role.id} />{labelName(labels, role.id)}</label>
        {/each}
      </fieldset>
    {/if}
    <label>What happened <textarea rows="2" bind:value={draft.outcome}></textarea></label>
    <label>My part <textarea rows="2" bind:value={draft.my_part}></textarea></label>
    <label>Others’ contributions <textarea rows="2" bind:value={draft.others_part}></textarea></label>
    <label>Help I used <textarea rows="2" bind:value={draft.help}></textarea></label>
    <label>How I checked <textarea rows="2" bind:value={draft.checking}></textarea></label>
    <label>Prior attempt
      <select aria-label="Prior attempt" bind:value={draft.prior_attempt_id}>
        <option value={null}>None</option>
        {#each entries.filter((entry) => !entry.withdrawn && entry.moment_id !== source && entry.primary_project_id === project) as entry (entry.moment_id)}
          <option value={entry.moment_id}>{sourceName(entry)}</option>
        {/each}
      </select>
    </label>
    <button type="submit" disabled={pending}>Save attempt</button>
  </form>
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if versions.length > 1}
    <details>
      <summary>Earlier attempt versions</summary>
      <ul>
        {#each versions.slice(1) as item (item.revision)}
          <li>
            <p>Version {item.revision} · {item.editor_name}{item.details.event ? ` · ${item.details.event}` : ''}</p>
            <button type="button" class="quiet" disabled={pending} onclick={() => save(undefined, item.revision)}>Restore attempt version {item.revision}</button>
          </li>
        {/each}
      </ul>
    </details>
  {/if}
{:else}
  <p role="status">Loading attempt…</p>
{/if}

<style>
  label { display: block; margin-block: .5rem; }
  form { padding-block: .75rem; }
</style>
