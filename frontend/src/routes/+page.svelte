<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$lib/shim/navigation';
  import { fetchMeta, fetchRoom, type RoomStatus } from '$lib/api';

  let code = $state('');
  let codeLength = $state(3);
  let liveRoom = $state<RoomStatus | null>(null);

  onMount(() => {
    fetchMeta()
      .then((meta) => {
        codeLength = meta.join_code_length || 3;
      })
      .catch(() => {});

    let stopped = false;
    const refreshRoom = () =>
      fetchRoom()
        .then((status) => {
          if (!stopped) liveRoom = status.live && status.code ? status : null;
        })
        .catch(() => {});
    void refreshRoom();

    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refreshRoom();
    }, 15_000);
    const onVisibility = () => {
      if (document.visibilityState === 'visible') void refreshRoom();
    };
    document.addEventListener('visibilitychange', onVisibility);

    return () => {
      stopped = true;
      window.clearInterval(timer);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  });

  function join(event: SubmitEvent): void {
    event.preventDefault();
    const clean = code
      .trim()
      .toUpperCase()
      .replace(/[^0-9]/g, '')
      .slice(0, 6);
    if (clean.length === codeLength) void goto(`/j/${clean}`);
  }
</script>

<main class="landing joy-bg">
  <div class="landing-inner">
    <div class="brand"><span class="brand-mark">✈</span> BIA · Triple A</div>

    <div>
      <h1>Quiz BIA</h1>
    </div>

    {#if liveRoom && liveRoom.code}
      <div class="live-room">
        <span class="live-dot" aria-hidden="true"></span>
        <div class="live-room-copy">
          <p class="live-room-label">Partie en cours</p>
          <p class="live-room-code mono">{liveRoom.code}</p>
        </div>
        <a class="btn btn-primary" href={liveRoom.join_url || `/j/${liveRoom.code}`}>
          Rejoindre la partie
        </a>
      </div>
    {/if}

    <div class="landing-grid">
      <form class="join-card stack" onsubmit={join}>
        <h2>Rejoindre une partie</h2>
        <input
          class="code-input"
          bind:value={code}
          maxlength={codeLength}
          inputmode="numeric"
          pattern="[0-9]*"
          autocomplete="off"
          placeholder={'0'.repeat(codeLength)}
          aria-label="Code de la partie"
        />
        <button class="btn btn-primary" type="submit">Rejoindre</button>
        <p class="small muted">
          Scannez le QR code affiché par le professeur, ou saisissez le code à {codeLength}
          chiffre{codeLength > 1 ? 's' : ''}.
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
