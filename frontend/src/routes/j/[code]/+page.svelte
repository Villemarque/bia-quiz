<script lang="ts">
  import { onMount } from 'svelte';
  import { QuizClient } from '$lib/stores/quiz.svelte';
  import { readLocal, writeLocal } from '$lib/env';
  import AnswerTile from '$lib/components/AnswerTile.svelte';

  let { data } = $props();
  const code = $derived(String(data?.code ?? '').toUpperCase());
  const client = new QuizClient('student');

  let nickname = $state(readLocal('bia-nickname'));
  let started = $state(false);

  onMount(() => {
    if (code && nickname.trim()) start(nickname);
    return () => client.destroy();
  });

  function start(name: string): void {
    const clean = name.trim().slice(0, 24);
    if (!clean || !code) return;
    nickname = clean;
    writeLocal('bia-nickname', clean);
    started = true;
    client.connectStudent(code, readLocal('bia-token'), clean);
  }

  function submit(event: SubmitEvent): void {
    event.preventDefault();
    start(nickname);
  }

  const timerWidth = $derived(
    client.question
      ? Math.max(
          0,
          Math.min(100, (client.remaining / (client.question.seconds * 1000)) * 100),
        )
      : 0,
  );

  const canAnswer = $derived(
    client.phase === 'question' && client.remaining > 0,
  );
</script>

<main class="student joy-bg">
  <header class="student-top">
    <span class="brand"><span class="brand-mark">✈</span> Quiz BIA</span>
    <span class="spacer"></span>
    {#if client.connected}
      <span class="pill pill-live">En ligne</span>
    {:else}
      <span class="pill">Connexion…</span>
    {/if}
  </header>

  {#if !code}
    <div class="card stack" style="margin:auto; max-width:420px; width:100%">
      <h1>Code manquant</h1>
      <p class="muted">Scannez le QR code affiché par le professeur.</p>
      <a class="btn btn-primary" href="/">Retour</a>
    </div>
  {:else if !started}
    <div class="card stack" style="margin:auto; max-width:420px; width:100%">
      <h1>Rejoindre</h1>
      <p class="muted">Partie <strong class="mono">{code}</strong></p>
      <form class="stack" onsubmit={submit}>
        <label for="nickname">Votre prénom</label>
        <input
          id="nickname"
          type="text"
          bind:value={nickname}
          maxlength="24"
          autocomplete="nickname"
          autocapitalize="words"
        />
        <button class="btn btn-primary" type="submit">Entrer</button>
      </form>
    </div>
  {:else if client.final}
    <div class="card stack" style="margin:auto; text-align:center; max-width:420px">
      <h1>Partie terminée</h1>
      <div class="big-rank">{client.final.my_rank ?? '—'}</div>
      <p class="mono">{client.final.my_points ?? 0} points</p>
      <p class="small muted">Bien joué, {nickname} !</p>
      <a class="btn btn-primary" href="/">Retour</a>
    </div>
  {:else if client.question}
    <div class="stack" style="flex:1">
      <div class="row">
        <span class="pill">{client.question.index + 1} / {client.question.total}</span>
        <span class="pill mono">{Math.ceil(client.remaining / 1000)} s</span>
        {#if client.paused}
          <span class="pill">En pause — vous pouvez répondre</span>
        {/if}
        <span class="spacer"></span>
        <span class="small muted">Regardez l'écran du professeur</span>
      </div>

      <div class="timer">
        <div class="timer-fill" style={`width:${timerWidth}%`}></div>
      </div>

      <div class="answer-grid">
        {#each client.question.choices as choice, index}
          <AnswerTile
            variant="student"
            {choice}
            index={index}
            selected={client.answer === index}
            disabled={!canAnswer}
            onclick={() => client.choose(index)}
          />
        {/each}
      </div>

      {#if client.answer !== null && client.result === null}
        <p class="small muted" style="text-align:center">
          Réponse envoyée — vous pouvez encore changer jusqu'à la fin du chrono.
        </p>
      {/if}

      {#if client.result !== null}
        <div class="result" class:good={client.result} class:bad={!client.result}>
          {client.result ? 'Bonne réponse' : 'Mauvaise réponse'}
        </div>
      {/if}
    </div>
  {:else}
    <div class="card stack" style="margin:auto; text-align:center; max-width:420px">
      <h1>En attente</h1>
      <p class="muted">{client.playerCount} joueur(s) connecté(s)</p>
      <p class="small">Le professeur va lancer la première question.</p>
      {#if client.error}
        <p class="error">{client.error}</p>
      {/if}
    </div>
  {/if}

  {#if client.error && client.question}
    <p class="error">{client.error}</p>
  {/if}
</main>
