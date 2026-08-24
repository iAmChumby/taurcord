# Taurcord

A lightweight Discord client with **Vencord** built on **Tauri v2**. All the mods, none of the Electron.

Taurcord runs Discord's full web app inside a native WebView2 window and injects the official
[Vencord](https://github.com/Vendicated/Vencord) browser build at document start — so you get the
complete Vencord experience (plugins, themes, QuickCSS, settings) in a native desktop shell that is
a fraction of the size of the Electron-based Discord client.

```
Taurcord installer:  ~1.2 MB        installed: ~4 MB
Discord installer:   ~100+ MB       installed: ~500 MB (Electron + Chromium bundled)
```

## Features

- Full Discord web experience: login (password / QR / passkey), DMs, servers, channels, voice
  chat, calls, streaming — everything that works on discord.com works here
- **Vencord 1.15.x** injected natively (no extension, no userscript manager):
  - 100+ plugins with the full plugin configuration UI
  - Themes (local QuickCSS and remote theme URLs)
  - Vencord settings section inside Discord's settings
- **Imports your existing desktop-Vencord setup on first launch** — enabled plugins, plugin
  settings, themes and QuickCSS are migrated from `%APPDATA%\Vencord` automatically
- **Custom themed titlebar** — frameless window with an in-app titlebar drawn from Discord's own
  CSS variables, so Vencord themes restyle the window chrome along with everything else. Drag,
  double-click maximize, and min/max/close all work natively.
- Voice chat ready out of the box: microphone, camera and notification permissions are granted
  automatically at the WebView2 level (no permission popups, no blocked mic)
- External links open in your real browser; Discord navigation stays in the app
- Single-instance: launching Taurcord twice focuses the existing window
- Per-user install (no admin/UAC required), clean uninstall

## Migrating from desktop Vencord

If you have the classic Vencord installed into the Discord desktop client, Taurcord imports it on
first launch:

- `settings.json` (enabled plugins + all plugin settings) → the app's settings store
- `themes/*.theme.css` → the theme library (enabled themes stay enabled)
- `settings/quickCss.css` → QuickCSS

The import runs once (flagged in the app's storage). To re-import after changing your desktop
Vencord setup, delete the `__taurcordVencordMigrated` key from the site's localStorage, or just
ask in an issue for a "re-import" button.

## Install

1. Grab `Taurcord_<version>_x64-setup.exe` from
   [Releases](https://github.com/iAmChumby/taurcord/releases) (or build it yourself, see below).
2. Run it. That's it — it installs per-user with a Start Menu shortcut.

Silent install:

```powershell
.\Taurcord_0.1.0_x64-setup.exe /S
```

## Uninstall

- **Settings → Apps → Installed apps → Taurcord → Uninstall**, or
- **Add/Remove Programs → Taurcord**, or

Silent uninstall:

```powershell
& "$env:LOCALAPPDATA\Taurcord\uninstall.exe" /S
```

## Build from source

Requirements: Windows 10/11, [Rust](https://rustup.rs) (MSVC toolchain), WebView2 Runtime
(preinstalled on Windows 11), Node.js (only for the Tauri CLI).

```powershell
git clone https://github.com/iAmChumby/taurcord.git
cd taurcord
npx @tauri-apps/cli build
# installer appears at target\release\bundle\nsis\Taurcord_<version>_x64-setup.exe
```

For a quick debug run: `cargo build` then `target\debug\taurcord.exe`.

### Updating Vencord

The Vencord browser build is vendored in `resources/vencord/` (source:
[Vencord/builds](https://github.com/Vencord/builds), file `extension-chrome.zip`, files
`dist/Vencord.js` + `dist/Vencord.css`). To update, download a fresh `extension-chrome.zip`,
replace those two files, and rebuild. See `resources/vencord/VERSION.txt` for the current version.

## How it works

1. A pure-Rust Tauri v2 app creates a single WebView2 window pointed at `https://discord.com/app`.
2. Two initialization scripts are injected at document start (before any Discord code runs):
   - `Vencord.js` — the official Vencord browser build, verbatim, in the page's main world
     (exactly what the Vencord Chrome extension does with its MAIN-world content script)
   - a small bridge that injects `Vencord.css`, and posts the `vencord:meta` message the browser
     build expects from its extension companion
3. A WebView2 `PermissionRequested` handler (registered via COM through
   `WebviewWindow::with_webview`) auto-allows microphone, camera and notifications so Discord
   voice works without prompts.
4. Navigation is restricted to `*.discord.com` / `discord.gg`; anything else (invite previews,
   links) opens in your default browser. Popups are denied.
5. Browser args enable autoplay without gesture (notification/voice sounds) and relax web
   security so Vencord can fetch themes from URLs (Discord's CSP would otherwise block them).

## Known limitations

- The Monaco-based QuickCSS editor loads its vendor assets from the extension package, which is
  not bundled; QuickCSS falls back to a plain editor. Themes and plugins are unaffected.
- Titlebar buttons don't show a hover highlight (they are native hit-zones, not DOM buttons), and
  Win11 snap-layouts-on-hover isn't available. Drag, double-click maximize, and all three buttons
  work.
- Vencord's global keybinds (toggle mute/deafen via extension commands) are unavailable — use
  Discord's in-app keybinds instead.
- Screen sharing uses the WebView2 capture stack; if a picker doesn't appear on your machine,
  use voice chat as usual and stream from the desktop client.
- `--disable-web-security` is passed to the webview (required for theme fetching). Only Discord
  content is ever loaded in the webview.

## License

GPL-3.0 — see [LICENSE](LICENSE). Vencord is © Vendicated and contributors, GPL-3.0; the vendored
build in `resources/vencord/` is unmodified from the official build archive.
