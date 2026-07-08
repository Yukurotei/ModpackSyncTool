# ModpackSync

A small desktop app for keeping a friend group's Minecraft mods folder in sync — no more "did you get the new jar?" texts.

One person (the **host**) publishes their mods folder to GitHub. Everyone else (**friends**) watches that repo and syncs the mod set into their own instance folder, whenever they want (or automatically). Friends can exclude individual mods (e.g. ones incompatible with their platform) and that exclusion sticks across future updates.

## Downloads

Grab the latest build for your platform from the [Releases page](../../releases):

| Platform | File |
|---|---|
| Windows | `.msi` or `.exe` |
| Linux (most distros) | `.AppImage` |
| Linux (Debian/Ubuntu) | `.deb` |
| macOS (Apple Silicon — M1/M2/M3/M4) | `ModpackSync_..._aarch64.dmg` |
| macOS (Intel) | `ModpackSync_..._x64.dmg` |

The app isn't code-signed (no Apple/Microsoft developer certificate), so:
- **Windows**: SmartScreen may warn about an unrecognized publisher — click "More info" → "Run anyway".
- **macOS**: Gatekeeper will say the app "is damaged and can't be opened" — this is just the quarantine flag on unsigned downloads, not actual corruption. Fix it once:
  ```
  xattr -cr /path/to/ModpackSync.app
  ```
  Then open normally (or right-click → Open the first time).
- **Linux on a bleeding-edge/rolling distro (e.g. Arch)**: if the AppImage shows a blank/gray window or crashes with an EGL error, your system's graphics stack is likely newer than what the AppImage's bundled WebKitGTK expects. [Build from source](#building-from-source) instead — it'll link your system's own WebKitGTK.

## Using it

### Hosting a modpack

1. Go to **Settings** and add a GitHub personal access token (see [below](#github-token-setup) for exactly what permissions it needs). The app validates it immediately and stores it in your OS keychain — never written to disk in plaintext.
2. Go to **Publish**. The first time, the app auto-creates a dedicated GitHub repo for you (`<your-username>/modpacksync-modpacks`) — nothing to set up manually. This is shown at the top of the Publish tab; share that `owner/repo` with friends so they know what to watch.
3. Pick your **instance folder** — the launcher folder that *contains* a `mods` subfolder (not the `mods` folder itself).
4. Give the modpack an ID (internal, lowercase-no-spaces) and a display name, then hit **Publish**.
5. To push an update later, open the **Modpack** dropdown at the top of the Publish tab, pick the existing one (this also refills the instance folder you used last time), and publish again — it bumps the version automatically.

### Syncing a modpack (friend side)

No GitHub account or token needed at all.

1. Go to **Sync**, enter the host's `owner/repo`, and click **Watch**.
2. Once a modpack shows up, click **Sync** — first time, it'll ask for your instance folder (same deal: the folder containing `mods`, not `mods` itself). It's remembered after that; use **Change folder** if you ever need to point it elsewhere.
3. Review the add/update/remove preview, then confirm.
4. Optionally, click **Manage mods** on a modpack to toggle individual mods off — excluded mods stay excluded across future syncs until you turn them back on.

The app checks for updates in the background every 20 minutes (and once right on launch) and notifies you when something changed — that doesn't require a GitHub token either, since it reads `index.json` anonymously.

### Auto-sync

**Settings → Auto-sync** (off by default). When enabled, the background check applies updates immediately instead of just notifying you — but only for modpacks you've already synced manually at least once (so the app knows where to put files). Brand-new modpacks still need one manual first sync.

## GitHub token setup

Only needed on the **host** side, to publish. The app needs to: create one repo (once), read/write `index.json`, and create releases + upload assets.

### Classic personal access token (simplest)

1. Go to **github.com → Settings → Developer settings → [Personal access tokens → Tokens (classic)](https://github.com/settings/tokens)**.
2. **Generate new token (classic)**.
3. Check the **`repo`** scope (the top-level checkbox — this covers repo creation, contents, and releases in one go; the sub-scopes don't need to be picked individually).
4. Generate, copy the token, paste it into ModpackSync's Settings.

### Fine-grained personal access token

1. Go to **github.com → Settings → Developer settings → [Personal access tokens → Fine-grained tokens](https://github.com/settings/personal-access-tokens)**.
2. **Generate new token**. Resource owner: your account.
3. **Repository access: "All repositories"** — the app creates a *new* repo on first publish, and you can't grant access to a repo that doesn't exist yet.
4. Under **Repository permissions**, set **Contents: Read and write**.
5. Under **Account permissions**, set **Administration: Read and write** — this is what allows the token to create a new repository for you (repo creation is an account-level action, not a per-repo one).
6. Generate, copy, paste into Settings.

If a fine-grained token gives a `403 Resource not accessible by personal access token` error when publishing, double check step 5 — that's almost always a missing Account-level Administration permission, not a Contents one.

## Building from source

Needed if the prebuilt Linux AppImage doesn't run on your system (see [Downloads](#downloads)), or if you just want to build it yourself.

**Prerequisites:**
- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) 20+
- Linux only: `webkit2gtk-4.1`, `libappindicator`/`libayatana-appindicator`, `librsvg` dev packages (Arch: `sudo pacman -S webkit2gtk-4.1 libappindicator-gtk3 librsvg`; Debian/Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`)

**Build:**
```sh
git clone https://github.com/Yukurotei/ModpackSyncTool.git
cd ModpackSyncTool
npm install
npm run tauri build
```

The finished binary/bundle ends up under `src-tauri/target/release/` (and `src-tauri/target/release/bundle/` for the packaged `.deb`/`.AppImage`/etc., if bundling succeeds on your system). On Linux, building this way links your system's own WebKitGTK instead of an older bundled one, which sidesteps AppImage compatibility issues entirely.

For quick local testing without a full release build:
```sh
npm run tauri dev
```
