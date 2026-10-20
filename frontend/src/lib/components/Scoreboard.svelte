<script lang="ts">
  import { onMount } from 'svelte';
  import type { Score } from '$lib/types';

  const DURATION = 1500;

  let {
    scores,
    title = 'Classement',
    from = {},
    previous = [],
    animate = false,
  }: {
    scores: Score[];
    title?: string;
    from?: Record<string, number>;
    previous?: Score[];
    animate?: boolean;
  } = $props();

  let scale = $state(1);
  let displayed = $state<Record<string, number>>({});
  let mounted = $state(false);

  const shifts = $derived.by(() => {
    const result: Record<string, number> = {};
    if (!animate || previous.length === 0) return result;
    scores.forEach((score, index) => {
      const oldIndex = previous.findIndex((entry) => entry.nickname === score.nickname);
      const oldRank = oldIndex >= 0 ? oldIndex : previous.length;
      result[score.nickname] = oldRank - index;
    });
    return result;
  });

  $effect(() => {
    const target = scores;
    const start = Object.fromEntries(
      target.map((score) => [score.nickname, from[score.nickname] ?? 0]),
    );
    scale = Math.max(1, ...target.map((score) => score.points), ...Object.values(start));
    displayed = Object.fromEntries(
      target.map((score) => [score.nickname, start[score.nickname] ?? 0]),
    );

    if (!animate) {
      displayed = Object.fromEntries(target.map((score) => [score.nickname, score.points]));
      return;
    }

    const begin = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      const t = Math.min(1, (now - begin) / DURATION);
      const eased = 1 - Math.pow(1 - t, 3);
      displayed = Object.fromEntries(
        target.map((score) => {
          const before = start[score.nickname] ?? 0;
          return [score.nickname, Math.round(before + (score.points - before) * eased)];
        }),
      );
      if (t < 1) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  });

  onMount(() => {
    const frame = requestAnimationFrame(() => (mounted = true));
    return () => cancelAnimationFrame(frame);
  });
</script>

{#if scores.length > 0}
  <section class="stack leaderboard-panel">
    <h2>{title}</h2>
    <ol class="leaderboard">
      {#each scores as score, index (score.nickname)}
        <li style={`--shift:${mounted ? 0 : (shifts[score.nickname] ?? 0)}`}>
          <span
            class="bar"
            style={`width:${Math.min(100, ((displayed[score.nickname] ?? 0) / scale) * 100)}%`}
          ></span>
          <span class="rank">{index + 1}</span>
          <span class="nickname">{score.nickname}</span>
          <span class="points">{displayed[score.nickname] ?? 0}</span>
        </li>
      {/each}
    </ol>
  </section>
{/if}
