<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$lib/shim/navigation';
  import { fetchMeta, type Meta } from '$lib/api';

  let code = $state('');
  let meta = $state<Meta | null>(null);

  onMount(() => {
    fetchMeta()
      .then((value) => {
        meta = value;
      })
      .catch(() => {});
  });

  function join(event: SubmitEvent): void {
    event.preventDefault();
    const clean = code
      .trim()
      .toUpperCase()
      .replace(/[^A-Z0-9]/g, '')
      .slice(0, 6);
    if (clean.length >= 4) void goto(`/j/${clean}`);
  }
</script>

<main class="landing">
  <div class="landing-inner">
    <div class="brand"><span class="brand-mark">✈</span> BIA · Triple A</div>

    <div>
      <h1>Quiz BIA</h1>
      <p class="lead">
        Le professeur projette la question, vous répondez depuis votre téléphone.
        Plus vite vous répondez, plus vous marquez de points.
      </p>
    </div>

    <div class="landing-grid">
      <form class="join-card stack" onsubmit={join}>
        <h2>Rejoindre une partie</h2>
        <input
          class="code-input"
          bind:value={code}
          maxlength="6"
          autocomplete="off"
          autocapitalize="characters"
          placeholder="CODE"
          aria-label="Code de la partie"
        />
        <button class="btn btn-primary" type="submit">Rejoindre</button>
        <p class="small muted">
          Scannez le QR code affiché par le professeur, ou saisissez le code à 6 caractères.
        </p>
      </form>

      <div class="join-card stack">
        <h2>Professeur</h2>
        <p class="small muted">
          Préparez une partie par chapitre, projetez les questions, lancez le chronomètre.
        </p>
        <a class="btn btn-ghost" href="/teacher">Espace professeur</a>
        <a class="btn btn-ghost" href="/prof">Résultats des parties</a>
      </div>
    </div>

    <footer class="small muted">
      BIA · Aérien Ambassadeurs Avenir — code sous AGPL-3.0 ·
      <a href="https://github.com/Villemarque/bia-annales-database-csv">banque de questions</a>
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
    </footer>
  </div>
</main>
