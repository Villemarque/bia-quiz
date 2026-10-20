<script lang="ts">
  import { onMount } from 'svelte';
  import QRCode from 'qrcode';
  import { QuizClient } from '$lib/stores/quiz.svelte';
  import { fetchCatalog } from '$lib/api';
  import { readSession, writeSession } from '$lib/env';
  import AnswerTile from '$lib/components/AnswerTile.svelte';
  import Scoreboard from '$lib/components/Scoreboard.svelte';
  import type { ChapterPick, SubjectCatalog } from '$lib/types';

  const client = new QuizClient('teacher');

  let subjects = $state<SubjectCatalog[]>([]);
  let selected = $state<string[]>([]);
  let picks = $state<Record<string, number>>({});
  let seconds = $state(30);
  let qr = $state('');
  let loadError = $state('');
  let settled = $state(false);
  let copied = $state(false);

  type PickItem = {
    key: string;
    subject: number;
    chapter: number | null;
    label: string;
    available: number;
  };

  const DEFAULT_PER_PICK = 10;

  const subjectGroups = $derived(
    subjects
      .map((subject) => {
        const items: PickItem[] = subject.chapters.map((chapter) => ({
          key: `${subject.subject}:${chapter.number}`,
          subject: subject.subject,
          chapter: chapter.number,
          label: chapter.title,
          available: chapter.question_count,
        }));
        if (subject.no_chapter_count > 0) {
          items.push({
            key: `${subject.subject}:none`,
            subject: subject.subject,
            chapter: null,
            label: 'Sans chapitre',
            available: subject.no_chapter_count,
          });
        }
        return { ...subject, items };
      })
      .filter((subject) => subject.question_count > 0),
  );

  const allItems = $derived(subjectGroups.flatMap((subject) => subject.items));

  onMount(() => {
    fetchCatalog()
      .then((rows) => {
        subjects = rows;
      })
      .catch(() => {
        loadError = 'Impossible de charger le catalogue (la base est-elle importée ?).';
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

  const shortJoinUrl = $derived(client.joinUrl.replace(/^https?:\/\//, ''));

  const totalPicked = $derived(
    selected.reduce((sum, key) => sum + (Number(picks[key]) || 0), 0),
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

  function toggleItem(item: PickItem): void {
    if (selected.includes(item.key)) {
      selected = selected.filter((key) => key !== item.key);
      delete picks[item.key];
      return;
    }
    selected = [...selected, item.key];
    picks[item.key] = Math.max(1, Math.min(DEFAULT_PER_PICK, item.available));
  }

  function create(): void {
    const choices: ChapterPick[] = selected
      .map((key) => allItems.find((item) => item.key === key))
      .filter((item): item is PickItem => item !== undefined)
      .map((item) => ({
        subject: item.subject,
        chapter: item.chapter,
        count: Math.max(0, Math.floor(Number(picks[item.key]) || 0)),
      }))
      .filter((pick) => pick.count > 0);
    if (choices.length === 0) return;
    client.create(choices, Number(seconds));
  }

  function endSession(): void {
    if (!window.confirm('Terminer la partie maintenant ? Le classement final sera affiché.')) {
      return;
    }
    client.end();
  }

  function restarted(): void {
    writeSession('bia-room', '');
    location.assign('/teacher');
  }

  async function copyJoinUrl(): Promise<void> {
    const url = client.joinUrl;
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      // Fallback for browsers without the async clipboard API.
      const area = document.createElement('textarea');
      area.value = url;
      area.style.position = 'fixed';
      area.style.opacity = '0';
      document.body.appendChild(area);
      area.select();
      document.execCommand('copy');
      area.remove();
    }
    copied = true;
    window.setTimeout(() => (copied = false), 2000);
  }
</script>

<main class="teacher joy-bg">
  <header class="teacher-bar">
    <span class="brand"><span class="brand-mark">✈</span> Quiz BIA</span>
    {#if client.joinUrl}
      <span class="join-bar">
        <span class="join-label">Rejoins à l'adresse</span>
        <a
          class="pill mono join-pill"
          href={client.joinUrl}
          target="_blank"
          rel="noreferrer"
          title="Ouvrir la page de connexion"
        >
          {shortJoinUrl}
        </a>
      </span>
    {:else if client.code}
      <span class="pill mono">Partie {client.code}</span>
    {/if}
    <span class="spacer"></span>
    {#if client.question}
      <span class="pill">{client.question.index + 1} / {client.question.total}</span>
    {/if}
    {#if !client.connected}
      <span class="pill">Hors ligne</span>
    {/if}

    <div class="teacher-bar-actions">
      {#if client.final}
        <button class="btn btn-primary" onclick={restarted}>Nouvelle partie</button>
      {:else if client.phase === 'reveal_leaderboard'}
        <button class="btn btn-primary" onclick={() => client.next()}>
          Question suivante
        </button>
        <button class="btn btn-ghost" onclick={endSession}>Terminer</button>
      {:else if client.question}
        {#if client.phase === 'question'}
          <button class="btn btn-primary" onclick={() => client.forceReveal()}>
            Révéler maintenant
          </button>
          {#if client.paused}
            <button
              class="btn btn-ghost btn-icon is-blinking"
              onclick={() => client.resume()}
              aria-label="Reprendre"
              title="Reprendre"
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M7 5l12 7-12 7z" />
              </svg>
            </button>
          {:else}
            <button
              class="btn btn-ghost btn-icon"
              onclick={() => client.pause()}
              aria-label="Mettre en pause"
              title="Mettre en pause"
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <rect x="6" y="5" width="4" height="14" rx="1" />
                <rect x="14" y="5" width="4" height="14" rx="1" />
              </svg>
            </button>
          {/if}
          <button
            class="btn btn-ghost btn-icon"
            onclick={() => client.skip()}
            aria-label="Passer"
            title="Passer"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M5 5l9 7-9 7z" />
              <rect x="16" y="5" width="3" height="14" rx="1" />
            </svg>
          </button>
        {:else}
          <button class="btn btn-primary" onclick={() => client.showLeaderboard()}>
            Afficher le classement
          </button>
        {/if}
        <button class="btn btn-ghost" onclick={endSession}>Terminer</button>
      {:else if client.roomSeen}
        <button class="btn btn-primary" onclick={() => client.startQuestion()}>
          Démarrer la question
        </button>
        <button class="btn btn-ghost" onclick={endSession}>Terminer</button>
      {:else if showSetup}
        <button
          class="btn btn-primary"
          disabled={selected.length === 0 || totalPicked < 1}
          onclick={create}
        >
          Créer la partie
        </button>
      {/if}
    </div>
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
      </div>
    {:else if client.phase === 'reveal_leaderboard'}
      <div class="stack leaderboard-screen">
        <h1>Classement</h1>
        <Scoreboard
          scores={client.leaderboard}
          previous={client.previousLeaderboard}
          from={client.previousPoints}
          animate
          title="Top 7"
        />
      </div>
    {:else if client.question}
      <div class="stack">
        <div class="row">
          {#if client.phase === 'question'}
            <span class="pill mono">{Math.ceil(client.remaining / 1000)} s</span>
            <div class="timer timer-inline">
              <div class="timer-fill" style={`width:${timerWidth}%`}></div>
            </div>
          {:else}
            <span class="spacer"></span>
          {/if}
          <span class="pill">{client.answeredCount} / {client.playerCount} réponses</span>
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
      </div>
    {:else if showSetup}
      <div class="stack" style="max-width: 900px">
        <h1>Nouvelle partie</h1>
        <p class="muted">
          Choisissez des chapitres, ou « Sans chapitre », le nombre de questions pour
          chaque sélection, puis la durée.
        </p>

        <div class="chapter-groups">
          {#each subjectGroups as subject}
            <section class="chapter-group">
              <h3>
                {subject.title}
                <span class="muted small">{subject.question_count} questions</span>
              </h3>
              <div class="chapter-list">
                {#each subject.items as item}
                  <div class="chapter-item" class:is-selected={selected.includes(item.key)}>
                    <label class="chapter-pick">
                      <input
                        type="checkbox"
                        checked={selected.includes(item.key)}
                        onchange={() => toggleItem(item)}
                      />
                      <span>{item.label}</span>
                    </label>
                    <span class="muted small" title="Questions disponibles">
                      {item.available}
                    </span>
                    {#if selected.includes(item.key)}
                      <label class="chapter-count" title="Questions à tirer">
                        <input
                          type="number"
                          min="1"
                          max={item.available}
                          step="1"
                          aria-label={`Nombre de questions pour ${item.label}`}
                          bind:value={picks[item.key]}
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
          <span class="spacer"></span>
          <label class="field">
            Secondes
            <input type="number" min="5" max="120" bind:value={seconds} />
          </label>
        </div>

        {#if selected.length > 0 && totalPicked === 0}
          <p class="muted small">Indiquez au moins une question.</p>
        {/if}
      </div>
    {:else if client.roomSeen}
      <div class="lobby-grid">
        <div class="stack" style="align-items:flex-start">
          <div class="join-link-card">
            <p class="join-link-label">Rejoindre sur</p>
            <p class="join-url mono">{client.joinUrl}</p>
            <button class="btn btn-primary join-copy" onclick={copyJoinUrl} aria-live="polite">
              {copied ? 'Lien copié !' : 'Copier le lien'}
            </button>
          </div>
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
        </div>
      </div>
    {:else}
      <p class="muted">Connexion au serveur…</p>
    {/if}
  </div>
</main>
