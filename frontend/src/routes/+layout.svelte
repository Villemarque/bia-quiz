<script lang="ts">
  import SourceFooter from '$lib/components/SourceFooter.svelte';

  let { children } = $props();
</script>

<div class="layout">
  <div class="layout-main">
    {@render children()}
  </div>
  <SourceFooter />
</div>

<style>
  .layout {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .layout-main {
    flex: 1 0 auto;
    display: flex;
    flex-direction: column;
  }
</style>
