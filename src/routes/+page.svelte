<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { fly, slide } from "svelte/transition";
  import Icon from "$lib/Icon.svelte";
  import Onboarding from "$lib/Onboarding.svelte";

  const ONBOARDED_KEY = "mps-onboarded";

  type Tab = "publish" | "sync" | "settings";
  let activeTab = $state<Tab>("sync");
  let showOnboarding = $state(
    typeof window !== "undefined" && !window.localStorage.getItem(ONBOARDED_KEY),
  );

  function finishOnboarding(role: "host" | "friend" | null) {
    window.localStorage.setItem(ONBOARDED_KEY, "1");
    showOnboarding = false;
    if (role === "host") activeTab = "publish";
    if (role === "friend") activeTab = "sync";
  }

  // ---------- Settings / token ----------
  let hasToken = $state(false);
  let tokenInput = $state("");
  let tokenStatus = $state("");
  let savingToken = $state(false);

  async function refreshTokenStatus() {
    hasToken = await invoke<boolean>("has_github_token");
  }
  refreshTokenStatus();

  async function saveToken(event: Event) {
    event.preventDefault();
    savingToken = true;
    tokenStatus = "Checking token...";
    try {
      const login = await invoke<string>("set_github_token", { token: tokenInput });
      tokenStatus = `Saved — authenticated as ${login}`;
      tokenInput = "";
      await refreshTokenStatus();
    } catch (e) {
      tokenStatus = `Error: ${e}`;
    } finally {
      savingToken = false;
    }
  }

  async function clearToken() {
    await invoke("clear_github_token");
    tokenStatus = "Token cleared";
    await refreshTokenStatus();
  }

  // ---------- Publish (host) ----------
  let instancePath = $state("");
  let modpackId = $state("");
  let modpackName = $state("");
  let description = $state("");
  let publishStatus = $state("");
  let publishing = $state(false);
  let publishRepo = $state<{ owner: string; repo: string } | null>(null);
  let publishRepoStatus = $state("");

  async function loadPublishRepo() {
    if (!hasToken) return;
    publishRepoStatus = "Setting up your modpack repo...";
    try {
      publishRepo = await invoke<{ owner: string; repo: string }>("get_or_create_publish_repo");
      publishRepoStatus = "";
    } catch (e) {
      publishRepoStatus = `Error: ${e}`;
    }
  }

  async function copyPublishRepo() {
    if (!publishRepo) return;
    await navigator.clipboard.writeText(`${publishRepo.owner}/${publishRepo.repo}`);
  }

  async function browseInstancePath() {
    const dir = await open({ directory: true, multiple: false, title: "Select your instance folder" });
    if (dir && !Array.isArray(dir)) instancePath = dir;
  }

  async function publish(event: Event) {
    event.preventDefault();
    publishing = true;
    publishStatus = "Publishing...";
    try {
      const result = await invoke<{
        tag: string;
        version: number;
        mod_count: number;
        release_url: string;
      }>("publish_modpack", {
        instancePath,
        modpackId,
        name: modpackName,
        description,
      });
      const repoLabel = publishRepo ? `${publishRepo.owner}/${publishRepo.repo}` : "your repo";
      publishStatus = `Published ${result.tag} — ${result.mod_count} mods. Friends watching ${repoLabel} will see it.`;
      await loadPublishRepo();
    } catch (e) {
      publishStatus = `Error: ${e}`;
    } finally {
      publishing = false;
    }
  }

  $effect(() => {
    if (activeTab === "publish" && hasToken && !publishRepo) {
      loadPublishRepo();
    }
  });

  // ---------- Sync (friend) ----------
  type WatchedRepo = { owner: string; repo: string };
  type CachedModpack = {
    owner: string;
    repo: string;
    modpack_id: string;
    name: string;
    description: string;
    latest_tag: string;
    latest_version: number;
    manifest_asset: string;
    mods_asset: string;
    updated_at: string;
  };
  type ModpackListItem = CachedModpack & { synced_version: number | null; excluded_count: number };
  type SyncPreview = {
    session_id: string;
    to_add: string[];
    to_update: string[];
    to_remove: string[];
    excluded: string[];
  };
  type ModpackFiles = {
    files: string[];
    excluded: string[];
    synced_files: string[];
    destination_path: string | null;
  };

  let watchRepoOwner = $state("");
  let watchRepoName = $state("");
  let watchedRepos = $state<WatchedRepo[]>([]);
  let modpacks = $state<ModpackListItem[]>([]);
  let watchStatus = $state("");
  let refreshing = $state(false);

  let syncPreviews = $state<Record<string, SyncPreview | undefined>>({});
  let syncBusy = $state<Record<string, boolean>>({});
  let syncStatus = $state<Record<string, string>>({});

  let modFilesOpen = $state<Record<string, boolean>>({});
  let modFiles = $state<Record<string, ModpackFiles | undefined>>({});
  let modFilesBusy = $state<Record<string, boolean>>({});
  let modFilesStatus = $state<Record<string, string>>({});

  function modpackKey(m: { owner: string; repo: string; modpack_id: string }) {
    return `${m.owner}/${m.repo}/${m.modpack_id}`;
  }

  async function loadWatchedRepos() {
    watchedRepos = await invoke<WatchedRepo[]>("list_watched_repos");
  }

  async function loadModpacks() {
    modpacks = await invoke<ModpackListItem[]>("list_modpacks");
  }

  loadWatchedRepos();
  loadModpacks();

  listen("modpacks-updated", () => {
    loadModpacks();
  });

  async function addWatchedRepo(event: Event) {
    event.preventDefault();
    watchStatus = "Adding...";
    try {
      await invoke("add_watched_repo", { owner: watchRepoOwner, repo: watchRepoName });
      await invoke("refresh_repo", { owner: watchRepoOwner, repo: watchRepoName });
      watchRepoOwner = "";
      watchRepoName = "";
      watchStatus = "";
      await loadWatchedRepos();
      await loadModpacks();
    } catch (e) {
      watchStatus = `Error: ${e}`;
    }
  }

  async function removeWatchedRepo(r: WatchedRepo) {
    await invoke("remove_watched_repo", { owner: r.owner, repo: r.repo });
    await loadWatchedRepos();
    await loadModpacks();
  }

  async function refreshAll() {
    refreshing = true;
    watchStatus = "Refreshing...";
    try {
      for (const r of watchedRepos) {
        await invoke("refresh_repo", { owner: r.owner, repo: r.repo });
      }
      await loadModpacks();
      watchStatus = "";
    } catch (e) {
      watchStatus = `Error: ${e}`;
    } finally {
      refreshing = false;
    }
  }

  function syncLabel(m: ModpackListItem): string {
    if (m.synced_version == null) return "Not synced";
    if (m.synced_version < m.latest_version) return `Update available — v${m.synced_version} → v${m.latest_version}`;
    return "Up to date";
  }

  function syncState(m: ModpackListItem): "none" | "update" | "current" {
    if (m.synced_version == null) return "none";
    if (m.synced_version < m.latest_version) return "update";
    return "current";
  }

  async function startSync(m: ModpackListItem) {
    const key = modpackKey(m);
    const destination = await open({ directory: true, multiple: false, title: `Sync destination for ${m.name}` });
    if (!destination || Array.isArray(destination)) return;

    syncBusy = { ...syncBusy, [key]: true };
    syncStatus = { ...syncStatus, [key]: "Fetching manifest and mod files..." };
    try {
      const preview = await invoke<SyncPreview>("preview_sync", {
        owner: m.owner,
        repo: m.repo,
        modpackId: m.modpack_id,
        destinationPath: destination,
      });
      syncPreviews = { ...syncPreviews, [key]: preview };
      syncStatus = { ...syncStatus, [key]: "" };
    } catch (e) {
      syncStatus = { ...syncStatus, [key]: `Error: ${e}` };
    } finally {
      syncBusy = { ...syncBusy, [key]: false };
    }
  }

  async function confirmSync(m: ModpackListItem) {
    const key = modpackKey(m);
    const preview = syncPreviews[key];
    if (!preview) return;
    syncBusy = { ...syncBusy, [key]: true };
    try {
      const result = await invoke<{ added: number; updated: number; removed: number }>(
        "apply_sync",
        { sessionId: preview.session_id },
      );
      syncStatus = {
        ...syncStatus,
        [key]: `Synced — +${result.added} updated ${result.updated} removed ${result.removed}`,
      };
      syncPreviews = { ...syncPreviews, [key]: undefined };
      await loadModpacks();
    } catch (e) {
      syncStatus = { ...syncStatus, [key]: `Error: ${e}` };
    } finally {
      syncBusy = { ...syncBusy, [key]: false };
    }
  }

  function cancelSync(m: ModpackListItem) {
    const key = modpackKey(m);
    syncPreviews = { ...syncPreviews, [key]: undefined };
  }

  async function loadModFiles(m: ModpackListItem) {
    const key = modpackKey(m);
    modFilesBusy = { ...modFilesBusy, [key]: true };
    modFilesStatus = { ...modFilesStatus, [key]: "" };
    try {
      const files = await invoke<ModpackFiles>("get_modpack_files", {
        owner: m.owner,
        repo: m.repo,
        modpackId: m.modpack_id,
      });
      modFiles = { ...modFiles, [key]: files };
    } catch (e) {
      modFilesStatus = { ...modFilesStatus, [key]: `Error: ${e}` };
    } finally {
      modFilesBusy = { ...modFilesBusy, [key]: false };
    }
  }

  async function toggleModsPanel(m: ModpackListItem) {
    const key = modpackKey(m);
    const isOpen = !modFilesOpen[key];
    modFilesOpen = { ...modFilesOpen, [key]: isOpen };
    if (isOpen && !modFiles[key]) {
      await loadModFiles(m);
    }
  }

  async function toggleExclusion(m: ModpackListItem, filename: string, currentlyExcluded: boolean) {
    const key = modpackKey(m);
    const willExclude = !currentlyExcluded;
    try {
      await invoke("set_exclusion", {
        owner: m.owner,
        repo: m.repo,
        modpackId: m.modpack_id,
        filename,
        excluded: willExclude,
      });

      const files = modFiles[key];
      if (willExclude && files?.synced_files.includes(filename)) {
        const deleteNow = confirm(
          `"${filename}" is currently synced into your mods folder. Delete it now as well? ` +
            `(If not, it'll be removed the next time you sync.)`,
        );
        if (deleteNow) {
          const deleted = await invoke<boolean>("delete_synced_file", {
            owner: m.owner,
            repo: m.repo,
            modpackId: m.modpack_id,
            filename,
          });
          modFilesStatus = {
            ...modFilesStatus,
            [key]: deleted ? `Deleted ${filename} from your mods folder.` : "",
          };
        }
      }

      await loadModFiles(m);
      await loadModpacks();
    } catch (e) {
      modFilesStatus = { ...modFilesStatus, [key]: `Error: ${e}` };
    }
  }

  function isError(msg: string | undefined): boolean {
    return !!msg && msg.startsWith("Error:");
  }
</script>

{#if showOnboarding}
  <Onboarding onfinish={finishOnboarding} />
{/if}

<div class="shell">
  <nav class="sidebar">
    <div class="brand">
      <span class="brand-mark"><Icon name="flame" size={16} /><Icon name="sparkle" size={16} /></span>
      <span class="brand-name">ModpackSync</span>
    </div>

    <button class="nav-item" class:active={activeTab === "publish"} data-accent="host" onclick={() => (activeTab = "publish")}>
      <Icon name="flame" size={17} />
      Publish
    </button>
    <button class="nav-item" class:active={activeTab === "sync"} data-accent="friend" onclick={() => (activeTab = "sync")}>
      <Icon name="sparkle" size={17} />
      Sync
    </button>
    <button class="nav-item" class:active={activeTab === "settings"} onclick={() => (activeTab = "settings")}>
      <Icon name="gear" size={17} />
      Settings
    </button>

    <button class="nav-item help" onclick={() => (showOnboarding = true)}>
      <Icon name="help" size={17} />
      How this works
    </button>
  </nav>

  <main class="content">
    {#key activeTab}
      <div class="page" in:fly={{ y: 6, duration: 200, delay: 60 }}>
        {#if activeTab === "publish"}
          <header class="page-header">
            <h1>Publish a modpack</h1>
            <p>Push your mods folder to GitHub so friends can grab it and get notified on updates.</p>
          </header>

          {#if !hasToken}
            <div class="banner warn">
              <Icon name="alert" size={16} />
              <span>You'll need a GitHub token before you can publish.</span>
              <button class="link" onclick={() => (activeTab = "settings")}>Add one in Settings</button>
            </div>
          {/if}

          {#if hasToken}
            <div class="card repo-banner">
              {#if publishRepo}
                <div class="repo-banner-text">
                  <span class="hint">Your modpacks live in</span>
                  <code>{publishRepo.owner}/{publishRepo.repo}</code>
                  <span class="hint">— share this with friends so they can watch it.</span>
                </div>
                <button type="button" class="secondary" onclick={copyPublishRepo}>
                  <Icon name="check" size={14} /> Copy
                </button>
              {:else}
                <span class="hint">{publishRepoStatus || "Setting up your modpack repo..."}</span>
              {/if}
            </div>
          {/if}

          <form class="card form" onsubmit={publish}>
            <div class="field">
              <label for="instancePath">Instance folder</label>
              <div class="field-row">
                <input id="instancePath" placeholder="Folder that contains your mods/ subfolder" bind:value={instancePath} required />
                <button type="button" class="secondary" onclick={browseInstancePath}>
                  <Icon name="folder" size={15} /> Browse
                </button>
              </div>
              <span class="hint">The launcher instance folder — the one with a <code>mods</code> folder inside it, not the mods folder itself.</span>
            </div>

            <div class="field-grid">
              <div class="field">
                <label for="modpackId">Modpack ID</label>
                <input id="modpackId" placeholder="dragonic-adventure" bind:value={modpackId} required />
                <span class="hint">Lowercase, no spaces — used internally.</span>
              </div>
              <div class="field">
                <label for="modpackName">Display name</label>
                <input id="modpackName" placeholder="Dragonic Adventure" bind:value={modpackName} required />
              </div>
            </div>

            <div class="field">
              <label for="description">Description</label>
              <input id="description" placeholder="Our exploration/tech pack" bind:value={description} />
            </div>

            <button type="submit" class="primary host-btn" disabled={publishing}>
              {#if publishing}Publishing...{:else}<Icon name="flame" size={16} /> Publish{/if}
            </button>
          </form>

          {#if publishStatus}
            <p class="status-line" class:error={isError(publishStatus)} transition:slide={{ duration: 160 }}>
              {publishStatus}
            </p>
          {/if}
        {:else if activeTab === "sync"}
          <header class="page-header">
            <h1>Modpacks</h1>
            <p>Watch a friend's repo, then sync it into any folder you choose.</p>
          </header>

          <div class="card">
            <form class="watch-form" onsubmit={addWatchedRepo}>
              <input placeholder="Repo owner (e.g. alex)" bind:value={watchRepoOwner} required />
              <input placeholder="Repo name (e.g. dragonic-adventure)" bind:value={watchRepoName} required />
              <button type="submit" class="primary friend-btn">
                <Icon name="plus" size={15} /> Watch
              </button>
            </form>
            {#if watchStatus}
              <p class="status-line" class:error={isError(watchStatus)}>{watchStatus}</p>
            {/if}

            {#if watchedRepos.length > 0}
              <ul class="chip-list">
                {#each watchedRepos as r (r.owner + '/' + r.repo)}
                  <li class="chip" transition:slide={{ duration: 150, axis: "x" }}>
                    {r.owner}/{r.repo}
                    <button type="button" class="chip-remove" onclick={() => removeWatchedRepo(r)} aria-label="Stop watching {r.owner}/{r.repo}">
                      <Icon name="x" size={12} />
                    </button>
                  </li>
                {/each}
              </ul>
              <button type="button" class="secondary refresh-all" onclick={refreshAll} disabled={refreshing}>
                <span class:spin={refreshing}><Icon name="refresh" size={15} /></span>
                {refreshing ? "Refreshing..." : "Refresh all"}
              </button>
            {:else}
              <p class="empty">Nothing watched yet — add a repo above to see modpacks show up here.</p>
            {/if}
          </div>

          {#if modpacks.length === 0}
            <div class="empty-state">
              <Icon name="package" size={28} />
              <p>No modpacks yet. Once you watch a repo with a published modpack, it'll appear here.</p>
            </div>
          {:else}
            <div class="modpack-list">
              {#each modpacks as m, i (modpackKey(m))}
                {@const key = modpackKey(m)}
                {@const state = syncState(m)}
                <div class="card modpack" in:fly={{ y: 8, duration: 220, delay: i * 40 }}>
                  <div class="modpack-header">
                    <div class="modpack-info">
                      <div class="modpack-title-row">
                        <strong>{m.name}</strong>
                        <span class="repo-tag">{m.owner}/{m.repo}</span>
                      </div>
                      <p class="modpack-desc">{m.description}</p>
                      <div class="pill-row">
                        <span class="pill" data-state={state}>
                          {#if state === "current"}<Icon name="check" size={12} />{/if}
                          {syncLabel(m)}
                        </span>
                        {#if m.excluded_count > 0}
                          <span class="pill neutral">{m.excluded_count} excluded</span>
                        {/if}
                      </div>
                    </div>
                    <div class="modpack-actions">
                      <button type="button" class="secondary" onclick={() => toggleModsPanel(m)}>
                        {modFilesOpen[key] ? "Hide mods" : "Manage mods"}
                      </button>
                      <button type="button" class="primary friend-btn" onclick={() => startSync(m)} disabled={syncBusy[key]}>
                        {syncBusy[key] ? "Working..." : "Sync"}
                      </button>
                    </div>
                  </div>

                  {#if syncStatus[key]}
                    <p class="status-line" class:error={isError(syncStatus[key])} transition:slide={{ duration: 150 }}>
                      {syncStatus[key]}
                    </p>
                  {/if}

                  {#if modFilesOpen[key]}
                    <div class="inset-panel" transition:slide={{ duration: 200 }}>
                      {#if modFilesBusy[key] && !modFiles[key]}
                        <p class="hint">Loading mod list...</p>
                      {:else if modFiles[key]}
                        {@const files = modFiles[key]}
                        <p class="hint">
                          Turn a mod off to exclude it from your syncs — it stays excluded even as the pack updates, until you turn it back on.
                        </p>
                        <ul class="mod-file-list">
                          {#each files.files as f (f)}
                            {@const isExcluded = files.excluded.includes(f)}
                            <li>
                              <span class="mod-file-name" class:excluded={isExcluded}>{f}</span>
                              <label class="switch">
                                <input
                                  type="checkbox"
                                  checked={!isExcluded}
                                  onchange={() => toggleExclusion(m, f, isExcluded)}
                                />
                                <span class="switch-track"><span class="switch-thumb"></span></span>
                              </label>
                            </li>
                          {/each}
                        </ul>
                        {#if modFilesStatus[key]}
                          <p class="status-line" class:error={isError(modFilesStatus[key])}>{modFilesStatus[key]}</p>
                        {/if}
                      {:else}
                        <p class="status-line error">{modFilesStatus[key] ?? "Couldn't load the mod list."}</p>
                      {/if}
                    </div>
                  {/if}

                  {#if syncPreviews[key]}
                    {@const preview = syncPreviews[key]}
                    <div class="inset-panel preview" transition:slide={{ duration: 200 }}>
                      <div class="stat-row">
                        <span class="stat add">+{preview.to_add.length} add</span>
                        <span class="stat update">{preview.to_update.length} update</span>
                        <span class="stat remove">-{preview.to_remove.length} remove</span>
                        {#if preview.excluded.length > 0}
                          <span class="stat neutral">{preview.excluded.length} skipped</span>
                        {/if}
                      </div>
                      <div class="preview-actions">
                        <button type="button" class="primary friend-btn" onclick={() => confirmSync(m)} disabled={syncBusy[key]}>
                          Confirm sync
                        </button>
                        <button type="button" class="secondary" onclick={() => cancelSync(m)}>Cancel</button>
                      </div>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        {:else if activeTab === "settings"}
          <header class="page-header">
            <h1>Settings</h1>
            <p>Your GitHub token is only needed to publish — friends syncing your packs never need one.</p>
          </header>

          <div class="card form">
            <div class="field">
              <label for="token">GitHub personal access token</label>
              <input id="token" type="password" placeholder="github_pat_..." bind:value={tokenInput} />
              <span class="hint">Needs Contents: Read and write on the repos you publish to. Stored in your OS keychain, never on disk.</span>
            </div>
            <div class="button-row">
              <button type="button" class="primary host-btn" onclick={saveToken} disabled={savingToken || !tokenInput}>
                {savingToken ? "Checking..." : "Save token"}
              </button>
              {#if hasToken}
                <button type="button" class="secondary" onclick={clearToken}>Clear token</button>
              {/if}
            </div>
            <p class="status-line" class:error={isError(tokenStatus)}>
              {tokenStatus || (hasToken ? "A token is configured." : "No token configured yet.")}
            </p>
          </div>
        {/if}
      </div>
    {/key}
  </main>
</div>

