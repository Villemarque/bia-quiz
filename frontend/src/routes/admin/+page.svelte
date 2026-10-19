<script lang="ts">
  import { onMount } from 'svelte';
  import {
    fetchAdminStats,
    fetchJob,
    fetchMe,
    startImport,
    type AdminStats,
    type ImportJob,
    type Me,
  } from '$lib/api';

  let me = $state<Me | null>(null);
  let stats = $state<AdminStats | null>(null);
  let job = $state<ImportJob | null>(null);
  let error = $state('');
  let busy = $state(false);

  let timer: number | null = null;

  onMount(() => {
    void init();
    timer = window.setInterval(() => void refresh(), 1500);
    return () => {
      if (timer !== null) window.clearInterval(timer);
    };
  });

  async function init(): Promise<void> {
    try {
      me = await fetchMe();
      stats = await fetchAdminStats();
      job = await fetchJob();
    } catch (cause) {
      error = String(cause);
    }
  }

  async function refresh(): Promise<void> {
    try {
      const next = await fetchJob();
      const wasRunning = job?.state === 'downloading' || job?.state === 'importing';
      job = next;
      if (wasRunning && (next.state === 'done' || next.state === 'failed')) {
        stats = await fetchAdminStats();
      }
    } catch {
      /* polling errors are ignored; the next tick retries */
    }
  }

  async function run(): Promise<void> {
    error = '';
    busy = true;
    try {
      await startImport();
      await refresh();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  function human(bytes: number): string {
    if (bytes < 1024) return `${bytes} o`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} Ko`;
    return `${(bytes / 1024 / 1024).toFixed(1)} Mo`;
  }

  function date(milliseconds: number | null): string {
    return milliseconds ? new Date(milliseconds).toLocaleString('fr-FR') : '—';
  }

  const running = $derived(job?.state === 'downloading' || job?.state === 'importing');
</script>

<main class="prof joy-bg">
  <div class="row">
    <div>
      <h1>Administration</h1>
      <p class="muted">
        Connecté comme <strong>{me?.username ?? '…'}</strong>
        {#if me?.role === 'admin'}<span class="badge">admin</span>{/if}
      </p>
    </div>
    <span class="spacer"></span>
    <a class="btn btn-ghost" href="/prof">
      Résultats
    </a>
    <form method="post" action="/logout">
      <button class="btn btn-ghost">
        Déconnexion
      </button>
    </form>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if stats}
    <section class="stack" style="margin-top:1.5rem">
      <div class="row">
        <div class="card" style="flex:1;min-width:140px"><strong>{stats.questions}</strong><br /><span class="muted">questions</span></div>
        <div class="card" style="flex:1;min-width:140px"><strong>{stats.active}</strong><br /><span class="muted">actives</span></div>
        <div class="card" style="flex:1;min-width:140px"><strong>{stats.figures}</strong><br /><span class="muted">figures</span></div>
        <div class="card" style="flex:1;min-width:140px"><strong>{stats.chapters}</strong><br /><span class="muted">chapitres</span></div>
        <div class="card" style="flex:1;min-width:140px"><strong>{human(stats.db_bytes)}</strong><br /><span class="muted">base SQLite</span></div>
      </div>

      <div class="card stack">
        <h2>Importer la banque de questions</h2>
        <p class="small muted">
          L'import télécharge la banque depuis la source, puis remplace les questions.
          Les questions absentes du fichier sont désactivées, pas supprimées.
        </p>

        <p class="small muted" style="overflow-wrap:anywhere">
          Source : <span class="mono">{stats.upstream_url}</span>
        </p>
        <div>
          <button class="btn btn-primary" disabled={busy || running} onclick={run}>
            {running ? 'Import en cours…' : "Lancer l'import"}
          </button>
        </div>
      </div>

      {#if job && job.state !== 'idle'}
        <div class="card stack">
          <div class="row">
            <h2>Import</h2>
            <span class="spacer"></span>
            <span class="badge" class:no={job.state === 'failed'}>
              {job.state === 'downloading'
                ? 'Téléchargement…'
                : job.state === 'importing'
                  ? 'Import…'
                  : job.state === 'done'
                    ? 'Terminé'
                    : 'Échec'}
            </span>
          </div>
          <p class="small muted">
            Source : {job.source ?? '—'} · démarré {date(job.started_at)} · terminé {date(job.finished_at)}
          </p>
          {#if job.done > 0}
            <p class="mono">{job.done} questions traitées</p>
          {/if}
          {#if job.error}
            <p class="error">{job.error}</p>
          {/if}
          {#if job.report}
            <div class="row">
              <span class="pill">lues {job.report.read}</span>
              <span class="pill">importées {job.report.imported}</span>
              <span class="pill">figures {job.report.figures}</span>
              <span class="pill">ignorées {job.report.skipped.length}</span>
            </div>
            {#if job.report.skipped.length > 0}
              <details>
                <summary>Lignes ignorées ({job.report.skipped.length})</summary>
                <table>
                  <thead><tr><th>#</th><th>Ligne</th><th>qid</th><th>Raison</th></tr></thead>
                  <tbody>
                    {#each job.report.skipped as row}
                      <tr>
                        <td>{row.record}</td>
                        <td>{row.line}</td>
                        <td class="mono">{row.qid || '—'}</td>
                        <td>{row.reason}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </details>
            {/if}
            <a href="/api/admin/import/report.txt">Télécharger le rapport</a>
          {/if}
        </div>
      {/if}

    </section>
  {:else}
    <p class="muted">Chargement…</p>
  {/if}
</main>
