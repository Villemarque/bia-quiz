<script lang="ts">
  let {
    status = 404,
    error = 'Page introuvable.',
  }: { status?: number; error?: string } = $props();
</script>

<main class="center joy-bg">
  <div class="stack" style="max-width: 40ch; align-items: center">
    <p class="big-rank">{status}</p>
    <p>{error}</p>
    <a class="btn btn-primary" href="/">Retour à l'accueil</a>
  </div>
</main>
