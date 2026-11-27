<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Session } from './identity';
  import { labelName, sourceName, sourceVoice, type Attempt, type Label, type TimelineEntry } from './projects';

  let { entry, session, labels, onfailure }: {
    entry: TimelineEntry; session: Session; labels: Label[]; onfailure: (error: unknown) => void;
  } = $props();
  let attempt = $state<Attempt | null>(null);

  onMount(() => {
    void api<Attempt[]>(`/moments/${entry.moment_id}/attempt`, 'GET', undefined, session.csrf_token)
      .then(([current]) => { attempt = current ?? null; }).catch(onfailure);
  });
</script>

<article>
  <h3>{attempt?.details.event ?? 'Attempt'}</h3>
  <p>{entry.activity_date ?? 'Activity date not recorded'} · {sourceVoice(entry)}</p>
  <p><a href={`/?moment=${entry.moment_id}`}>{sourceName(entry)}</a></p>
  {#if attempt}
    {@const item = attempt.details}
    <dl>
      {#if item.event_type_id}<dt>Event type</dt><dd>{labelName(labels, item.event_type_id)}</dd>{/if}
      {#if item.role_ids.length}<dt>Roles</dt><dd>{item.role_ids.map((id) => labelName(labels, id)).sort().join(', ')}</dd>{/if}
      {#if item.outcome}<dt>What happened</dt><dd>{item.outcome}</dd>{/if}
      {#if item.my_part}<dt>My part</dt><dd>{item.my_part}</dd>{/if}
      {#if item.others_part}<dt>Others’ contributions</dt><dd>{item.others_part}</dd>{/if}
      {#if item.help}<dt>Help I used</dt><dd>{item.help}</dd>{/if}
      {#if item.checking}<dt>How I checked</dt><dd>{item.checking}</dd>{/if}
    </dl>
    {#if item.prior_attempt_id}<p><a href={`/?moment=${item.prior_attempt_id}`}>Open prior attempt’s moment</a></p>{/if}
    <p>Attempt details recorded by {attempt.editor_name}</p>
  {/if}
</article>

<style>
  article { padding: 1rem; border: 1px solid #b8bbae; border-radius: .4rem; overflow-wrap: anywhere; }
  h3, p { margin-block: 0 .7rem; }
  dt { font-weight: 650; }
  dd { margin: 0 0 .6rem; white-space: pre-wrap; }
</style>
