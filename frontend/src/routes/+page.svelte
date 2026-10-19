<script lang="ts">
  import { goto } from '$lib/shim/navigation';

  let code = $state('');

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
        <h2>Créer une nouvelle partie</h2>
        <p class="small muted">
          Préparez une partie par chapitre, projetez les questions, lancez le chronomètre.
        </p>
        <a class="btn btn-primary" href="/teacher">Créer une nouvelle partie</a>
      </div>
    </div>
  </div>
</main>
