<script lang="ts">
  import { onMount } from 'svelte';
  import { fetchMeta, type Meta } from '$lib/api';

  const REPO = 'https://github.com/Villemarque/bia-quiz';
  const LICENSE = 'https://www.gnu.org/licenses/agpl-3.0.html';
  const year = new Date().getFullYear();

  let meta = $state<Meta | null>(null);

  onMount(() => {
    fetchMeta()
      .then((value) => {
        meta = value;
      })
      .catch(() => {});
  });
</script>

<footer class="site-footer">
  <span>© {year} Solal Lorcy</span>
  <span aria-hidden="true">·</span>
  <a href={LICENSE}>AGPL-3.0</a>
  <span aria-hidden="true">·</span>
  <a href={meta?.source_url || REPO}>GitHub</a>
  {#if meta?.commit && meta.commit !== 'unknown'}
    <span aria-hidden="true">·</span>
    {#if meta.commit_url}
      <a href={meta.commit_url}>commit {meta.commit}</a>
    {:else}
      <span>commit {meta.commit}</span>
    {/if}
  {/if}
</footer>
