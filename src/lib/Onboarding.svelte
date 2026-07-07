<script lang="ts">
  import { fly, fade } from "svelte/transition";
  import Icon from "./Icon.svelte";

  let { onfinish }: { onfinish: (role: "host" | "friend" | null) => void } = $props();

  let step = $state(0);
  let role = $state<"host" | "friend" | null>(null);
  let direction = $state(1);

  function go(to: number) {
    direction = to > step ? 1 : -1;
    step = to;
  }

  function pick(r: "host" | "friend") {
    role = r;
    go(2);
  }

  function finish() {
    onfinish(role);
  }

  function skip() {
    onfinish(null);
  }
</script>

<div class="overlay" transition:fade={{ duration: 180 }}>
  <div class="card">
    <button class="skip" onclick={skip} aria-label="Skip walkthrough">
      <Icon name="x" size={16} />
    </button>

    {#key step}
      <div class="step" in:fly={{ x: direction * 24, duration: 260, delay: 120 }} out:fly={{ x: direction * -24, duration: 120 }}>
        {#if step === 0}
          <div class="eyebrow">Welcome</div>
          <h1>ModpackSync</h1>
          <p class="lede">
            Your friend group's mods folder, kept in sync — no more "did you get the
            new jar?" texts. One of you publishes, everyone else just clicks Sync.
          </p>
          <button class="primary" onclick={() => go(1)}>
            Get started <Icon name="arrow-right" size={16} />
          </button>
        {:else if step === 1}
          <div class="eyebrow">One question</div>
          <h1>What brings you here?</h1>
          <p class="lede">You can always do both later — this just picks where to start.</p>
          <div class="role-grid">
            <button class="role-card host" onclick={() => pick("host")}>
              <span class="role-icon"><Icon name="flame" size={26} /></span>
              <span class="role-title">Host a modpack</span>
              <span class="role-desc">Share your mods folder so friends can grab it and stay updated.</span>
            </button>
            <button class="role-card friend" onclick={() => pick("friend")}>
              <span class="role-icon"><Icon name="sparkle" size={26} /></span>
              <span class="role-title">Join a friend's pack</span>
              <span class="role-desc">Watch a modpack someone else is hosting and keep your mods matching it.</span>
            </button>
          </div>
        {:else if step === 2 && role === "host"}
          <div class="eyebrow host-text">Hosting</div>
          <h1>Three steps to publish</h1>
          <ol class="steps">
            <li><strong>Add a token.</strong> In Settings, save a GitHub personal access token — that's what lets the app publish on your behalf.</li>
            <li><strong>Point at your mods folder.</strong> Pick the instance folder your mods live in.</li>
            <li><strong>Name it and publish.</strong> Re-publish any time you add or remove mods — friends get notified.</li>
          </ol>
          <button class="primary host-btn" onclick={finish}>
            Take me to Settings <Icon name="arrow-right" size={16} />
          </button>
        {:else if step === 2 && role === "friend"}
          <div class="eyebrow friend-text">Joining</div>
          <h1>Three steps to sync</h1>
          <ol class="steps">
            <li><strong>Get the repo name.</strong> Ask whoever's hosting for their GitHub owner/repo, like <code>alex/dragonic-adventure</code>.</li>
            <li><strong>Add it under Sync.</strong> The app checks it in the background and tells you when there's something new.</li>
            <li><strong>Click Sync.</strong> Pick your mods folder, review what'll change, and confirm.</li>
          </ol>
          <button class="primary friend-btn" onclick={finish}>
            Take me to Sync <Icon name="arrow-right" size={16} />
          </button>
        {/if}
      </div>
    {/key}

    {#if step > 0}
      <div class="dots">
        {#each [0, 1, 2] as d}
          <span class="dot" class:active={d <= step}></span>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 1.5rem;
  }

  .card {
    position: relative;
    width: min(560px, 100%);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-md);
    padding: 2.5rem;
    overflow: hidden;
  }

  .skip {
    position: absolute;
    top: 1rem;
    right: 1rem;
    background: transparent;
    border: none;
    color: var(--text-dim);
    padding: 0.4rem;
    border-radius: 999px;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }

  .skip:hover {
    background: var(--surface-2);
    color: var(--text);
  }

  .step {
    min-height: 220px;
  }

  .eyebrow {
    font-family: var(--font-display);
    font-size: 0.7rem;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-dim);
    margin-bottom: 0.5rem;
  }

  .eyebrow.host-text {
    color: var(--host);
  }

  .eyebrow.friend-text {
    color: var(--friend);
  }

  h1 {
    font-family: var(--font-display);
    font-size: 1.6rem;
    margin: 0 0 0.75rem;
    letter-spacing: -0.01em;
  }

  .lede {
    color: var(--text-dim);
    line-height: 1.55;
    margin: 0 0 1.75rem;
    max-width: 44ch;
  }

  .primary {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    background: var(--text);
    color: var(--bg);
    border: none;
    padding: 0.7rem 1.2rem;
    border-radius: var(--radius-md);
    font-size: 0.95rem;
    font-weight: 600;
    transition: transform 0.15s var(--ease), box-shadow 0.15s var(--ease);
  }

  .primary:hover {
    transform: translateY(-1px);
  }

  .primary.host-btn {
    background: var(--host);
    color: #241505;
  }

  .primary.host-btn:hover {
    box-shadow: var(--shadow-glow-host);
  }

  .primary.friend-btn {
    background: var(--friend);
    color: #04211f;
  }

  .primary.friend-btn:hover {
    box-shadow: var(--shadow-glow-friend);
  }

  .role-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.85rem;
  }

  .role-card {
    text-align: left;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    transition: transform 0.15s var(--ease), border-color 0.15s var(--ease), box-shadow 0.15s var(--ease);
  }

  .role-card:hover {
    transform: translateY(-2px);
  }

  .role-card.host:hover {
    border-color: var(--host);
    box-shadow: var(--shadow-glow-host);
  }

  .role-card.friend:hover {
    border-color: var(--friend);
    box-shadow: var(--shadow-glow-friend);
  }

  .role-icon {
    color: var(--text-dim);
  }

  .role-card.host .role-icon {
    color: var(--host);
  }

  .role-card.friend .role-icon {
    color: var(--friend);
  }

  .role-title {
    font-weight: 700;
    font-size: 1.02rem;
  }

  .role-desc {
    font-size: 0.85rem;
    color: var(--text-dim);
    line-height: 1.45;
  }

  .steps {
    margin: 0 0 1.75rem;
    padding-left: 1.2rem;
    color: var(--text-dim);
    line-height: 1.6;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .steps strong {
    color: var(--text);
  }

  .steps code {
    background: var(--surface-2);
    border-radius: 4px;
    padding: 0.1rem 0.35rem;
    font-family: var(--font-display);
    font-size: 0.85em;
  }

  .dots {
    display: flex;
    gap: 0.4rem;
    justify-content: center;
    margin-top: 1.75rem;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--border);
    transition: background 0.2s var(--ease);
  }

  .dot.active {
    background: var(--text-dim);
  }

  @media (max-width: 520px) {
    .role-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
