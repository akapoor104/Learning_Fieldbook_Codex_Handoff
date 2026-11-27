<script lang="ts">
  import { api, type Session } from './identity';
  import { labelKinds, type Label, type LabelKind } from './projects';

  let { session, labels, onchanged, onfailure }: {
    session: Session; labels: Label[]; onchanged: () => Promise<void>; onfailure: (error: unknown) => void;
  } = $props();

  let kind = $state<LabelKind>('domain');
  let name = $state('');
  let key = crypto.randomUUID();
  let editing = $state<Label | null>(null);
  let pending = $state(false);

  function select(item: Label) {
    editing = item;
    name = item.name;
    kind = item.kind;
  }

  function reset() {
    editing = null;
    name = '';
    key = crypto.randomUUID();
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (pending) return;
    pending = true;
    try {
      await api(`/household-labels/${editing?.id ?? key}`, 'PUT',
        { name, kind, expected_name: editing?.name ?? null }, session.csrf_token);
      reset();
      await onchanged();
    } catch (failure) {
      onfailure(failure);
    } finally {
      pending = false;
    }
  }

  async function change(item: Label) {
    if (pending) return;
    pending = true;
    try {
      await api(`/household-labels/${item.id}${item.retired ? '/restore' : ''}`,
        item.retired ? 'POST' : 'DELETE', undefined, session.csrf_token);
      await onchanged();
    } catch (failure) {
      onfailure(failure);
    } finally {
      pending = false;
    }
  }
</script>

<details>
  <summary>Household labels</summary>
  <form onsubmit={save}>
    <label>Label type
      <select aria-label="Label type" bind:value={kind} disabled={!!editing || pending}>
        {#each Object.entries(labelKinds) as [value, title] (value)}
          <option {value}>{title}</option>
        {/each}
      </select>
    </label>
    <label>Label name <input required maxlength="200" bind:value={name} disabled={pending} /></label>
    <button type="submit" disabled={pending}>{editing ? 'Save label name' : 'Add label'}</button>
    {#if editing}<button type="button" class="quiet" onclick={reset}>Cancel rename</button>{/if}
  </form>
  <ul aria-label="Household label list">
    {#each labels.filter((item) => item.kind === kind) as item (item.id)}
      <li>
        <p>{item.name}{item.retired ? ' (retired)' : ''}</p>
        <button type="button" class="quiet" disabled={pending} onclick={() => select(item)}>Rename {item.name}</button>
        <button type="button" class="quiet" disabled={pending} onclick={() => change(item)}>
          {item.retired ? 'Restore' : 'Retire'} {item.name}
        </button>
      </li>
    {/each}
  </ul>
</details>

<style>
  label { display: block; margin-block: .5rem; }
  li { margin-block: .5rem; }
</style>
