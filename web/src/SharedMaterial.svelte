<script lang="ts">
  import type { SharedMoment } from './shares';
  let { item, download = false, preview = false }: { item: SharedMoment; download?: boolean; preview?: boolean } = $props();
</script>

<p>
  {#if item.voice === 'scribed'}{item.learner_name}’s words, entered by {item.recorder_name}
  {:else if item.voice === 'guardian'}Observation by {item.recorder_name}
  {:else}{item.learner_name}{/if}
  {#if item.activity_date} · {item.activity_date}{/if}
</p>
{#if item.note}<p class="words">{item.note}</p>{/if}
{#if item.originals.length}
  <ul aria-label="Selected originals">
    {#each item.originals as original (original.id)}
      <li>
        {#if download}
          <a href={`/shares/${item.id}/media/${original.id}`} download>{original.label}</a>
        {:else if preview}
          <a href={`/media/${original.id}`} download>{original.label}</a>
        {:else}{original.label}{/if}
        <small> Original · Uploaded by {original.uploader_name}</small>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .words { white-space: pre-wrap; overflow-wrap: anywhere; }
</style>
