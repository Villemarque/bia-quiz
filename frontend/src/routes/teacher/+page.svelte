<script lang="ts">
  import { onMount } from 'svelte';
  import QRCode from 'qrcode';
  import { QuizClient } from '$lib/stores/quiz.svelte';
  import { fetchChapters } from '$lib/api';
  import { readSession, writeSession } from '$lib/env';
  import AnswerTile from '$lib/components/AnswerTile.svelte';
  import Scoreboard from '$lib/components/Scoreboard.svelte';
  import type { Chapter } from '$lib/types';

  const client = new QuizClient('teacher');

  let chapters = $state<Chapter[]>([]);
  let selected = $state<number[]>([]);
  let picks = $state<Record<number, number>>({});
  let seconds = $state(30);
  let qr = $state('');
  let loadError = $state('');
  let settled = $state(false);

  type ChapterGroup = { label: string; numbers?: number[]; from?: number; to?: number };
  const GROUP_DEFS: ChapterGroup[] = [
    { label: 'Aéronefs', from: 0, to: 2 },
    { label: 'Mécanique du vol', from: 3, to: 5 },
    { label: 'Météo', from: 6, to: 11 },
    { label: 'Navigation', from: 12, to: 14 },
    { label: 'Histoire', numbers: [16] },
    { label: 'Anglais', numbers: [17] },
    { label: 'Sans chapitre', numbers: [15] },
  ];

  const DEFAULT_PER_CHAPTER = 10;

  const groupedChapters = $derived(
    GROUP_DEFS.map((group) => {
      const items = chapters.filter(
        (chapter) =>
          chapter.question_count > 0 &&
          (group.numbers
            ? group.numbers.includes(chapter.number)
            : chapter.number >= (group.from ?? 0) && chapter.number <= (group.to ?? -1)),
      );
      return {
        label: group.label,
        items,
        total: items.reduce((sum, chapter) => sum + chapter.question_count, 0),
      };
    }).filter((group) => group.total > 0),
  );

  onMount(() => {
    fetchChapters()
      .then((rows) => {
        chapters = rows;
      })
      .catch(() => {
        loadError = 'Impossible de charger les chapitres (la base est-elle importée ?).';
      });
    client.connectTeacher(readSession('bia-room') || undefined);
    const timer = window.setTimeout(() => {
      settled = true;
      if (!client.roomSeen) client.code = '';
    }, 900);
    return () => {
      window.clearTimeout(timer);
      client.destroy();
    };
  });

  $effect(() => {
    if (!client.joinUrl) {
      qr = '';
      return;
    }
    QRCode.toDataURL(client.joinUrl, {
      margin: 1,
      width: 400,
      color: { dark: '#071037', light: '#ffffff' },
    })
      .then((url) => {
        qr = url;
      })
      .catch(() => {
        qr = '';
      });
  });

  $effect(() => {
    if (client.roomSeen && client.code) writeSession('bia-room', client.code);
    if (client.phase === 'podium') writeSession('bia-room', '');
  });

  const showSetup = $derived(settled && !client.roomSeen && client.phase !== 'podium');

  const totalPicked = $derived(
    selected.reduce((sum, number) => sum + (Number(picks[number]) || 0), 0),
  );

  const timerWidth = $derived(
    client.question
      ? Math.max(
          0,
          Math.min(100, (client.remaining / (client.question.seconds * 1000)) * 100),
        )
      : 0,
  );

  const maxCount = $derived(
    client.reveal ? Math.max(1, ...client.reveal.counts) : 1,
  );

  function toggleChapter(chapter: Chapter): void {
    if (selected.includes(chapter.number)) {
      selected = selected.filter((value) => value !== chapter.number);
      delete picks[chapter.number];
      return;
    }
    selected = [...selected, chapter.number];
    picks[chapter.number] = Math.max(
      1,
      Math.min(DEFAULT_PER_CHAPTER, chapter.question_count),
    );
  }

  function create(): void {
    const chapters = selected
      .map((number) => ({
        number,
        count: Math.max(0, Math.floor(Number(picks[number]) || 0)),
      }))
      .filter((pick) => pick.count > 0);
    if (chapters.length === 0 || totalPicked > 50) return;
    client.create(chapters, Number(seconds));
  }

  function restarted(): void {
    writeSession('bia-room', '');
    location.assign('/teacher');
  }
</script>

<main class="teacher">
  <header class="teacher-bar">
    <span class="brand"><span class="brand-mark">✈</span> Quiz BIA</span>
    <span class="spacer"></span>
    {#if client.code}
      <span class="pill mono">Partie {client.code}</span>
    {/if}
    {#if client.question}
      <span class="pill">{client.question.index + 1} / {client.question.total}</span>
    {/if}
    <span class="pill">{client.connected ? 'En ligne' : 'Hors ligne'}</span>
  </header>

  <div class="teacher-main">
    {#if client.error}
      <div class="error">{client.error}</div>
    {/if}
    {#if loadError}
      <div class="error">{loadError}</div>
    {/if}

    {#if client.final}
      <div class="stack">
        <h1>Podium</h1>
        <div class="podium">
          {#each [{ index: 1, cls: 'second' }, { index: 0, cls: 'first' }, { index: 2, cls: 'third' }] as slot}
            {#if client.final.podium[slot.index]}
              <div class="podium-place {slot.cls}">
                <div class="podium-name">{client.final.podium[slot.index].nickname}</div>
                <div class="podium-block">{client.final.podium[slot.index].points}</div>
              </div>
            {/if}
          {/each}
        </div>
        <Scoreboard scores={client.final.top} title="Classement final (top 7)" />
        <div>
          <button class="btn btn-primary" onclick={restarted}>Nouvelle partie</button>
        </div>
      </div>
    {:else if client.question}
      <div class="stack">
        <div class="row">
          <span class="pill">{client.question.index + 1} / {client.question.total}</span>
          <span class="pill mono">{Math.ceil(client.remaining / 1000)} s</span>
          {#if client.paused}
            <span class="pill">En pause</span>
          {/if}
          <span class="spacer"></span>
          <span class="pill">{client.answeredCount} / {client.playerCount} réponses</span>
        </div>

        <div class="timer">
          <div class="timer-fill" style={`width:${timerWidth}%`}></div>
        </div>

        <p class="question-text">{client.question.text}</p>

        {#if client.question.figure_url}
          <div class="figure-wrap">
            <img
              class="figure"
              src={client.question.figure_url}
              alt="Figure associée à la question"
            />
          </div>
        {/if}

        <div class="choices">
          {#each client.question.choices as choice, index}
            <AnswerTile
              variant="teacher"
              {choice}
              index={index}
              correct={client.reveal?.correct_index === index}
              dim={client.reveal !== null && client.reveal.correct_index !== index}
            />
          {/each}
        </div>

        {#if client.reveal}
          <div class="count-bars">
            {#each client.question.choices as choice, index}
              <div class="count-bar">
                <span>{choice.letter}</span>
                <div class="count-track">
                  <div
                    class="count-fill"
                    style={`width:${(client.reveal.counts[index] / maxCount) * 100}%`}
                  ></div>
                </div>
                <span class="mono">{client.reveal.counts[index]}</span>
              </div>
            {/each}
          </div>
          <p class="correct-note">Bonne réponse : {client.reveal.correct_text}</p>
        {/if}

        <div class="teacher-controls">
          {#if client.phase === 'question'}
            <button class="btn btn-primary" onclick={() => client.forceReveal()}>
              Révéler maintenant
            </button>
            {#if client.paused}
              <button class="btn btn-ghost" onclick={() => client.resume()}>Reprendre</button>
            {:else}
              <button class="btn btn-ghost" onclick={() => client.pause()}>Pause</button>
            {/if}
            <button class="btn btn-ghost" onclick={() => client.skip()}>Passer</button>
          {:else}
            <button class="btn btn-primary" onclick={() => client.next()}>
              Question suivante
            </button>
          {/if}
          <span class="spacer"></span>
          <button class="btn btn-ghost" onclick={() => client.end()}>Terminer</button>
        </div>

        <Scoreboard scores={client.leaderboard} title="Top 7" />
      </div>
    {:else if showSetup}
      <div class="stack" style="max-width: 900px">
        <h1>Nouvelle partie</h1>
        <p class="muted">
          Choisissez un ou plusieurs chapitres, le nombre de questions pour chacun, puis
          la durée.
        </p>

        <div class="chapter-groups">
          {#each groupedChapters as group}
            <section class="chapter-group">
              <h3>
                {group.label}
                <span class="muted small">{group.total} questions</span>
              </h3>
              <div class="chapter-list">
                {#each group.items as chapter}
                  <div class="chapter-item" class:is-selected={selected.includes(chapter.number)}>
                    <label class="chapter-pick">
                      <input
                        type="checkbox"
                        checked={selected.includes(chapter.number)}
                        onchange={() => toggleChapter(chapter)}
                      />
                      <span>{chapter.title}</span>
                    </label>
                    <span class="muted small" title="Questions disponibles">
                      {chapter.question_count}
                    </span>
                    {#if selected.includes(chapter.number)}
                      <label class="chapter-count" title="Questions à tirer pour ce chapitre">
                        <input
                          type="number"
                          min="1"
                          max={Math.min(50, chapter.question_count)}
                          step="1"
                          aria-label={`Nombre de questions pour ${chapter.title}`}
                          bind:value={picks[chapter.number]}
                        />
                        <span class="muted small">q.</span>
                      </label>
                    {/if}
                  </div>
                {/each}
              </div>
            </section>
          {/each}
        </div>

        <div class="row setup-total">
          <span class="pill">
            Total : {totalPicked} question{totalPicked > 1 ? 's' : ''}
          </span>
          <span class="muted small">Maximum 50</span>
          <span class="spacer"></span>
          <label class="field">
            Secondes
            <input type="number" min="5" max="120" bind:value={seconds} />
          </label>
        </div>

        {#if selected.length > 0 && totalPicked === 0}
          <p class="muted small">Indiquez au moins une question.</p>
        {:else if totalPicked > 50}
          <div class="notice">
            Maximum 50 questions par partie ({totalPicked} demandées).
          </div>
        {/if}

        <div>
          <button
            class="btn btn-primary"
            disabled={selected.length === 0 || totalPicked < 1 || totalPicked > 50}
            onclick={create}
          >
            Créer la partie
          </button>
        </div>
      </div>
    {:else if client.roomSeen}
      <div class="lobby-grid">
        <div class="stack" style="align-items:flex-start">
          <p class="muted">Rejoindre sur</p>
          <p class="mono" style="overflow-wrap:anywhere">{client.joinUrl}</p>
          <div class="code-big">{client.code}</div>
          {#if qr}
            <img class="qr" src={qr} alt="QR code pour rejoindre la partie" />
          {/if}
        </div>

        <div class="stack">
          <h2>{client.playerCount} joueur(s)</h2>
          <div class="player-chips">
            {#each client.players as player}
              <span class="chip">{player.nickname}</span>
            {/each}
          </div>

          {#if client.warning}
            <div class="notice">{client.warning}</div>
          {/if}

          <div class="teacher-controls">
            <button class="btn btn-primary" onclick={() => client.startQuestion()}>
              Démarrer la question
            </button>
            <button class="btn btn-ghost" onclick={() => client.end()}>Terminer</button>
          </div>
        </div>
      </div>
    {:else}
      <p class="muted">Connexion au serveur…</p>
    {/if}
  </div>
</main>
