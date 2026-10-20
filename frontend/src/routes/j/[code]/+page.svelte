<script lang="ts">
  import { onMount } from 'svelte';
  import { QuizClient } from '$lib/stores/quiz.svelte';
  import { readLocal, writeLocal } from '$lib/env';
  import { fetchRoom } from '$lib/api';
  import AnswerTile from '$lib/components/AnswerTile.svelte';

  let { data } = $props();
  const code = $derived(String(data?.code ?? '').toUpperCase());
  const client = new QuizClient('student');

  let nickname = $state(readLocal('bia-nickname'));
  let started = $state(false);
  let checking = $state(true);
  let roomMissing = $state(false);

  onMount(() => {
    let stopped = false;
    fetchRoom()
      .then((status) => {
        if (stopped) return;
        if (!status.live || status.code !== code) {
          roomMissing = true;
          return;
        }
        checking = false;
        if (code && nickname.trim()) start(nickname);
      })
      .catch(() => {
        if (stopped) return;
        checking = false;
        if (code && nickname.trim()) start(nickname);
      });
    return () => {
      stopped = true;
      client.destroy();
    };
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

  const revealed = $derived(
    client.result !== null ||
      client.phase === 'reveal_answer' ||
      client.phase === 'reveal_leaderboard',
  );

  const selectedChoice = $derived(
    client.answer !== null ? (client.question?.choices[client.answer] ?? null) : null,
  );
</script>

<main class="student joy-bg">
  <header class="student-top">
    <span class="brand"><span class="brand-mark">✈</span> Quiz BIA</span>
  </header>

  {#if !code}
    <div class="card stack" style="margin:auto; max-width:420px; width:100%">
      <h1>Code manquant</h1>
      <p class="muted">Scannez le QR code affiché par le professeur.</p>
      <a class="btn btn-primary" href="/">Retour</a>
    </div>
  {:else if roomMissing}
    <div class="card stack" style="margin:auto; text-align:center; max-width:420px; width:100%">
      <h1>Partie introuvable</h1>
      <p class="muted">
        Aucune partie en cours avec le code <strong class="mono">{code}</strong>.
      </p>
      <a class="btn btn-primary" href="/">Retour à l'accueil</a>
    </div>
  {:else if checking}
    <div class="card stack" style="margin:auto; text-align:center; max-width:420px; width:100%">
      <p class="muted">Vérification de la partie…</p>
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

      {#if revealed}
        <div
          class="reveal-panel"
          class:good={client.result === true}
          class:bad={client.result === false && client.answer !== null}
        >
          <div class="reveal-headline">
            {#if client.result === true}
              Bonne réponse
            {:else if client.answer !== null}
              Mauvaise réponse
            {:else}
              Pas de réponse
            {/if}
          </div>
          <p class="reveal-choice">
            {#if selectedChoice}
              Votre réponse : <strong>{selectedChoice.letter}</strong> · {selectedChoice.text}
            {:else}
              Vous n'avez pas répondu.
            {/if}
          </p>
        </div>
      {:else}
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
