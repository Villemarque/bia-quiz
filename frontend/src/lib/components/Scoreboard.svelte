<script lang="ts">
  import type { Score } from '$lib/types';

  let { scores, title = 'Classement' }: { scores: Score[]; title?: string } = $props();
</script>

{#if scores.length > 0}
  <section class="stack">
    <h2>{title}</h2>
    <ol class="leaderboard">
      {#each scores as score, index}
        <li>
          <span class="rank">{index + 1}</span>
          <span>{score.nickname}</span>
          <span class="points">{score.points}</span>
        </li>
      {/each}
    </ol>
  </section>
{/if}
