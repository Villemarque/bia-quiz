<script lang="ts">
  import type { Choice } from '$lib/types';

  interface Props {
    choice: Choice;
    index: number;
    variant?: 'teacher' | 'student';
    selected?: boolean;
    correct?: boolean;
    dim?: boolean;
    disabled?: boolean;
    onclick?: () => void;
  }

  let {
    choice,
    index,
    variant = 'teacher',
    selected = false,
    correct = false,
    dim = false,
    disabled = false,
    onclick,
  }: Props = $props();

  const shapeLabels: Record<string, string> = {
    triangle: 'triangle',
    diamond: 'losange',
    circle: 'cercle',
    square: 'carré',
  };
</script>

{#if variant === 'student'}
  <button
    type="button"
    class="answer answer-{index}"
    class:is-selected={selected}
    {disabled}
    aria-pressed={selected}
    aria-label={`Réponse ${choice.letter}, ${shapeLabels[choice.shape] ?? choice.shape}`}
    {onclick}
  >
    <span class="shape shape-{choice.shape}" aria-hidden="true"></span>
    <span class="letter">{choice.letter}</span>
  </button>
{:else}
  <div class="choice choice-{index}" class:is-correct={correct} class:is-dim={dim}>
    <span class="shape shape-{choice.shape}" aria-hidden="true"></span>
    <span class="choice-text">{choice.text}</span>
    <span class="choice-badge">{choice.letter}</span>
  </div>
{/if}
