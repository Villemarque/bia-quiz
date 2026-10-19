<script lang="ts">
  import { onMount } from 'svelte';
  import {
    fetchMe,
    fetchSession,
    fetchSessions,
    type Me,
    type SessionDetail,
    type SessionListRow,
  } from '$lib/api';

  let me = $state<Me | null>(null);
  let sessions = $state<SessionListRow[]>([]);
  let detail = $state<SessionDetail | null>(null);
  let error = $state('');
  let loading = $state(true);

  onMount(() => {
    void load();
  });

  async function load(): Promise<void> {
    loading = true;
    error = '';
    try {
      me = await fetchMe();
      sessions = await fetchSessions();
    } catch {
      error = 'Authentification requise ou serveur indisponible.';
    } finally {
      loading = false;
    }
  }

  async function open(id: number): Promise<void> {
    error = '';
    try {
      detail = await fetchSession(id);
    } catch {
      error = 'Impossible de charger cette partie.';
    }
  }

  function date(milliseconds: number): string {
    return new Date(milliseconds).toLocaleString('fr-FR', {
      dateStyle: 'short',
      timeStyle: 'short',
    });
  }

  function order(raw: string): number[] {
    return raw.split(',').map((part) => Number(part.trim()));
  }

  function correctLetter(question: { display_order: string; answer_idx: number }): string {
    const position = order(question.display_order).indexOf(question.answer_idx);
    return 'ABCD'[position] ?? '?';
  }

  function choiceLetter(
    question: { display_order: string },
    originalIndex: number,
  ): string {
    const position = order(question.display_order).indexOf(originalIndex);
    return 'ABCD'[position] ?? '?';
  }
</script>

<main class="prof joy-bg">
  <div class="row">
    <div>
      <h1>Résultats des parties</h1>
      <p class="muted">
        Connecté comme <strong>{me?.username ?? '…'}</strong>
      </p>
    </div>
    <span class="spacer"></span>
    {#if me?.role === 'admin'}
      <a class="btn btn-ghost" href="/admin">
        Administration
      </a>
    {/if}
    <button class="btn btn-ghost" onclick={() => void load()}>
      Rafraîchir
    </button>
    <form method="post" action="/logout">
      <button class="btn btn-ghost">
        Déconnexion
      </button>
    </form>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p class="muted">Chargement…</p>
  {:else if sessions.length === 0}
    <p class="muted">Aucune partie enregistrée.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Date</th>
          <th>Code</th>
          <th>Joueurs</th>
          <th>Questions</th>
          <th>Durée / question</th>
          <th>État</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each sessions as session}
          <tr class="session-row" onclick={() => void open(session.id)}>
            <td>{date(session.started_at)}</td>
            <td class="mono">{session.code}</td>
            <td>{session.players}</td>
            <td>{session.question_count}</td>
            <td>{session.seconds_per_question} s</td>
            <td>{session.ended_at ? 'terminée' : 'interrompue'}</td>
            <td>
              <a
                href={`/api/prof/sessions/${session.id}/export.csv`}
                onclick={(event) => event.stopPropagation()}
              >
                CSV
              </a>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if detail}
    <section class="detail card" style="color: var(--navy)">
      <div class="row">
        <h2>Partie {detail.session.code}</h2>
        <span class="spacer"></span>
        <a class="btn btn-ghost" href={`/api/prof/sessions/${detail.session.id}/export.csv`}>
          Exporter en CSV
        </a>
      </div>

      <h3>Questions</h3>
      {#if detail.questions.length === 0}
        <p class="muted">Aucune question enregistrée.</p>
      {:else}
        <table>
          <thead>
            <tr>
              <th>#</th>
              <th>Énoncé</th>
              <th>Bonne réponse</th>
            </tr>
          </thead>
          <tbody>
            {#each detail.questions as question}
              <tr>
                <td>{question.position + 1}</td>
                <td>{question.text}</td>
                <td class="mono">{correctLetter(question)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

      <h3 style="margin-top:1.5rem">Réponses</h3>
      {#if detail.answers.length === 0}
        <p class="muted">Aucune réponse enregistrée.</p>
      {:else}
        <table>
          <thead>
            <tr>
              <th>Joueur</th>
              <th>Question</th>
              <th>Réponse</th>
              <th>Correcte</th>
              <th>Points</th>
            </tr>
          </thead>
          <tbody>
            {#each detail.answers as answer}
              {@const question = detail.questions.find((q) => q.position === answer.position)}
              <tr>
                <td>{answer.nickname}</td>
                <td>{answer.position + 1}</td>
                <td class="mono">{question ? choiceLetter(question, answer.choice_idx) : '?'}</td>
                <td>
                  {#if answer.is_correct}
                    <span class="badge">oui</span>
                  {:else}
                    <span class="badge no">non</span>
                  {/if}
                </td>
                <td class="mono">{answer.points}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  {/if}
</main>
