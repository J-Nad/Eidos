# Eidos

Eidos is a Windows-only native Tauri 2 desktop application with a floating command bar and a file-focused creation workspace.......

It builds to a native Windows MSI installer with:

- a compact translucent command bar toggled by `Ctrl+Space`;
- on-demand fixed-drive indexing with Rust, `jwalk`, SQLite, FTS5, and an in-memory cache;
- direct Windows file/folder opening through the default associated app;
- a decorated Split View window for previews, editing, folders, images, PDFs, and metadata;
- optional Gemini 3.1 Flash Lite AI assistance when `GEMINI_API_KEY` is configured.

No macOS or Linux target is supported in this branch.

## Prerequisites

- Windows 10 or Windows 11
- Node.js 20+ available on PATH
- Rust stable with the `x86_64-pc-windows-msvc` toolchain
- Visual Studio Build Tools 2022 with “Desktop development with C++”
- Windows 10/11 SDK

## Optional Gemini configuration

AI features are disabled unless this environment variable exists before Eidos starts:

```powershell
$env:GEMINI_API_KEY = 'your_google_ai_key'
```

When the key is missing, the Conductor chat pane is hidden and Void file creation returns a clear “AI unavailable” message. Search, indexing, preview, open, and save flows still work.

All AI requests use:

```text
gemini-3.1-flash-lite
https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite:streamGenerateContent
```

## Build

```powershell
npm.cmd install --legacy-peer-deps
npm.cmd run tauri build
```

The MSI is emitted to:

```text
src-tauri/target/release/bundle/msi/Eidos_1.0.0_x64_en-US.msi
```

The `--legacy-peer-deps` flag is currently needed because `svelte-french-toast` has stale peer metadata for Svelte 5, although it passes the Svelte 5 compiler and production build.

## Development

```powershell
npm.cmd run tauri dev
```

The Tauri dev server uses `http://localhost:5173`.

## Runtime behavior

- The database is stored at `%APPDATA%\Eidos\eidos.db`.
- Launch is intentionally light: the app loads the existing SQLite index into an in-memory cache, but does not crawl drives until the first search or “Index Now”.
- First search starts indexing all fixed Windows drives in the background.
- `Ctrl+Space` toggles the Eidos command bar.
- Left-clicking a result opens it with the Windows default app.
- Ctrl-clicking or middle-clicking a result opens Split View.
- Pressing Enter with zero results triggers Void creation if Gemini is configured.

See [HANDOVER.md](HANDOVER.md) for the full engineering handover.

## Release validation

Run the complete local release gate before packaging:

```powershell
npm.cmd run release:check
npm.cmd audit --omit=dev
npm.cmd run tauri build
```

The release gate checks Svelte and TypeScript diagnostics, builds the static frontend, runs the Rust test suite, and exercises Windows DPAPI and Windows Search integration tests.

Production dependencies currently report zero known vulnerabilities. Before public distribution, sign the generated executable and MSI with your organization’s Windows code-signing certificate. Keep the certificate and private key outside this repository and CI logs.
