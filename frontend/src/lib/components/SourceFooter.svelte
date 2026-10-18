<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchMeta, type Meta } from '$lib/api';

  let meta = $state<Meta | null>(null);

  onMount(() => {
    fetchMeta()
      .then((value) => {
        meta = value;
      })
      .catch(() => {});
  });
</script>

<p class="small muted" style="margin-top:2rem">
  Code source sous AGPL-3.0
  {#if meta?.source_url}
    · <a href={meta.source_url}>code source</a>
  {/if}
  {#if meta?.commit && meta.commit !== 'unknown'}
    · {#if meta.commit_url}
      <a href={meta.commit_url}>commit {meta.commit}</a>
    {:else}
      commit {meta.commit}
    {/if}
  {/if}
</p>
