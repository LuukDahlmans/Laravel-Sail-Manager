<script lang="ts">
  import { fade } from 'svelte/transition';
  import Icon from './Icon.svelte';

  type Phase = 'docker' | 'projects' | 'ready' | null;
  let { phase = null }: { phase?: Phase } = $props();

  const phaseLabel = $derived(
    phase === 'docker'
      ? 'Connecting to Docker'
      : phase === 'projects'
        ? 'Loading projects'
        : phase === 'ready'
          ? 'Ready'
          : 'Starting up',
  );
</script>

<div class="splash" data-tauri-drag-region out:fade={{ duration: 350 }}>
  <div class="content">
    <div class="logo-wrap">
      <span class="ring r1"></span>
      <span class="ring r2"></span>
      <div class="logo">
        <Icon name="waves" size={30} />
      </div>
    </div>

    <div class="brand-name">Sail Manager</div>
    <div class="tagline">Run many Laravel Sail projects in parallel</div>

    <div class="phase">
      <span class="dots">
        <span></span><span></span><span></span>
      </span>
      <span class="phase-text">{phaseLabel}</span>
    </div>
  </div>
</div>

<style>
  .splash {
    position: fixed;
    inset: 0;
    z-index: 9999;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg);
    /* Subtle accent halo so it doesn't read as flat. */
    background-image: radial-gradient(
      60% 50% at 50% 38%,
      var(--accent-soft) 0%,
      transparent 65%
    );
  }

  .content {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 12px;
  }

  .logo-wrap {
    position: relative;
    width: 96px;
    height: 96px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: 6px;
  }

  .logo {
    width: 60px;
    height: 60px;
    border-radius: 16px;
    background: linear-gradient(135deg, var(--accent) 0%, var(--accent-hover) 100%);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.18) inset,
      0 0 0 1px rgba(255, 255, 255, 0.06),
      0 10px 32px var(--accent-glow);
    animation: bob 2.6s ease-in-out infinite;
    position: relative;
    z-index: 2;
  }

  .ring {
    position: absolute;
    width: 60px;
    height: 60px;
    border-radius: 50%;
    border: 1.5px solid var(--accent);
    opacity: 0;
    animation: pulse 2.6s ease-out infinite;
    z-index: 1;
  }
  .ring.r2 {
    animation-delay: 1.3s;
  }

  .brand-name {
    font-size: 19px;
    font-weight: 650;
    letter-spacing: -0.01em;
    color: var(--text);
  }

  .tagline {
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-dim);
    max-width: 280px;
  }

  .phase {
    margin-top: 22px;
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-faint);
    font-size: 11px;
    letter-spacing: 0.02em;
  }

  .phase-text::after {
    content: '…';
    margin-left: 1px;
  }

  .dots {
    display: inline-flex;
    gap: 4px;
    align-items: center;
  }

  .dots span {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.35;
    animation: bounce 1.1s ease-in-out infinite;
  }
  .dots span:nth-child(2) {
    animation-delay: 0.15s;
  }
  .dots span:nth-child(3) {
    animation-delay: 0.3s;
  }

  @keyframes bob {
    0%,
    100% {
      transform: translateY(0);
    }
    50% {
      transform: translateY(-4px);
    }
  }

  @keyframes pulse {
    0% {
      transform: scale(0.85);
      opacity: 0.45;
    }
    100% {
      transform: scale(1.7);
      opacity: 0;
    }
  }

  @keyframes bounce {
    0%,
    100% {
      opacity: 0.3;
      transform: translateY(0);
    }
    50% {
      opacity: 1;
      transform: translateY(-3px);
    }
  }
</style>
