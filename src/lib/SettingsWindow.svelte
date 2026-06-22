<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { FolderOpen, HardDrive, KeyRound, LoaderCircle, Power, Save, X } from 'lucide-svelte';
  import { toast } from 'svelte-french-toast';
  import ToastProvider from '$lib/ToastProvider.svelte';
  import type { DriveEntry, Settings } from '$lib/types';

  const appWindow = getCurrentWindow();
  let drives: DriveEntry[] = [];
  let settings: Settings = {
    selectedDrives: [],
    defaultSaveLocation: '',
    autostart: false,
    geminiApiKey: ''
  };
  let saving = false;

  function toggleDrive(path: string) {
    if (settings.selectedDrives.includes(path)) {
      settings = { ...settings, selectedDrives: settings.selectedDrives.filter((drive) => drive !== path) };
    } else {
      settings = { ...settings, selectedDrives: [...settings.selectedDrives, path] };
    }
  }

  async function save() {
    if (settings.selectedDrives.length === 0) {
      toast.error('Choose at least one drive to index.');
      return;
    }
    saving = true;
    const id = toast.loading('Saving settings…');
    try {
      await invoke('set_settings', { settings });
      toast.success('Settings saved.', { id });
    } catch (error) {
      toast.error(String(error), { id });
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    const blockContextMenu = (event: MouseEvent) => event.preventDefault();
    document.documentElement.classList.remove('dark');
    document.addEventListener('contextmenu', blockContextMenu);
    void (async () => {
      drives = await invoke<DriveEntry[]>('available_drives');
      settings = await invoke<Settings>('get_settings');
    })().catch((error) => toast.error(String(error)));
    return () => {
      document.removeEventListener('contextmenu', blockContextMenu);
    };
  });
</script>

<main>
  <header>
    <div>
      <strong>Settings</strong>
      <span>Personalize Eidos</span>
    </div>
    <button class="close-button" on:click={() => appWindow.hide()} title="Close"><X size={16} /></button>
  </header>

  <div class="content">
    <section>
      <h2>Search locations</h2>
      <div class="card drive-list">
        {#each drives as drive (drive.path)}
          <label class="setting-row">
            <span class="symbol"><HardDrive size={18} strokeWidth={1.7} /></span>
            <span class="row-copy"><strong>{drive.label}</strong><small>Include in Eidos results</small></span>
            <input class="check" type="checkbox" checked={settings.selectedDrives.includes(drive.path)} on:change={() => toggleDrive(drive.path)} />
          </label>
        {/each}
      </div>
    </section>

    <section>
      <h2>Files</h2>
      <div class="card field-card">
        <span class="symbol"><FolderOpen size={18} strokeWidth={1.7} /></span>
        <label><span>Default save location</span><input class="text-input" bind:value={settings.defaultSaveLocation} spellcheck="false" /></label>
      </div>
    </section>

    <section>
      <h2>General</h2>
      <div class="card">
        <label class="setting-row">
          <span class="symbol"><Power size={18} strokeWidth={1.7} /></span>
          <span class="row-copy"><strong>Open at login</strong><small>Keep Eidos one shortcut away</small></span>
          <span class="toggle"><input type="checkbox" bind:checked={settings.autostart} /><i></i></span>
        </label>
      </div>
    </section>

    <section>
      <h2>Intelligence</h2>
      <div class="card field-card">
        <span class="symbol violet"><KeyRound size={18} strokeWidth={1.7} /></span>
        <label><span>Gemini API key</span><input class="text-input" type="password" bind:value={settings.geminiApiKey} placeholder="Optional · protected by Windows" autocomplete="off" /></label>
      </div>
      <p class="hint">Encrypted for your Windows account and never exposed to the frontend bundle.</p>
    </section>
  </div>

  <footer>
    <button class="primary-button" on:click={save} disabled={saving}>
      {#if saving}<LoaderCircle class="spin" size={14} />{:else}<Save size={14} />{/if}
      Save Changes
    </button>
  </footer>
  <ToastProvider />
</main>

<style>
  main { width: 100vw; height: 100vh; overflow: hidden; display: grid; grid-template-rows: 74px 1fr 68px; color: #26383f; background: linear-gradient(145deg,#f7fbfc,#eaf4f7); font-family: "SF Pro Text","Segoe UI Variable","Segoe UI",sans-serif; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 0 22px; border-bottom: 1px solid rgba(67,91,100,.12); background: rgba(255,255,255,.5); backdrop-filter: blur(22px); }
  header strong, header span { display: block; }
  header strong { font-size: 18px; font-weight: 650; letter-spacing: -.02em; }
  header span { margin-top: 2px; color: #73858c; font-size: 11px; }
  .close-button { width: 30px; height: 30px; display: grid; place-items: center; border: 0; border-radius: 50%; color: #657980; background: rgba(91,116,125,.1); cursor: pointer; transition: transform 140ms ease,background 140ms ease; }
  .close-button:hover { background: rgba(91,116,125,.17); } .close-button:active { transform: scale(.9); }
  .content { min-height: 0; overflow-y: auto; padding: 20px 22px; }
  section + section { margin-top: 18px; }
  h2 { margin: 0 0 7px 12px; color: #6c7e85; font-size: 11px; font-weight: 600; letter-spacing: .02em; }
  .card { overflow: hidden; border: 1px solid rgba(74,104,115,.14); border-radius: 16px; background: rgba(255,255,255,.78); box-shadow: 0 8px 24px rgba(40,67,76,.07); backdrop-filter: blur(18px); }
  .setting-row { min-height: 58px; display: flex; align-items: center; gap: 12px; padding: 8px 14px; }
  .setting-row + .setting-row { border-top: 1px solid rgba(74,104,115,.1); }
  .symbol { width: 32px; height: 32px; display: grid; place-items: center; flex: 0 0 32px; border-radius: 9px; color: #397e9a; background: #dff3fa; }
  .symbol.violet { color: #7259bb; background: #eee9ff; }
  .row-copy { min-width: 0; flex: 1; } .row-copy strong,.row-copy small { display:block; }
  .row-copy strong { font-size: 13px; font-weight: 550; } .row-copy small { margin-top:2px;color:#7d8d93;font-size:10px; }
  .check { width: 17px; height: 17px; accent-color: #2b9ed0; }
  .field-card { display:flex; align-items:center; gap:12px; padding:12px 14px; }
  .field-card label { min-width:0; flex:1; color:#52666e; font-size:11px; }
  .text-input { width: 100%; height: 32px; margin-top:5px; padding:0 10px; border:1px solid rgba(70,99,109,.14); border-radius:9px; outline:none; color:#2c4149; background:rgba(247,251,252,.9); font-size:11px; transition:border 140ms ease,box-shadow 140ms ease; }
  .text-input:focus { border-color:rgba(28,145,198,.55); box-shadow:0 0 0 3px rgba(28,145,198,.11); }
  .toggle { position:relative;width:38px;height:22px;flex:0 0 38px; }
  .toggle input { position:absolute;inset:0;opacity:0;cursor:pointer; }
  .toggle i { display:block;width:38px;height:22px;border-radius:999px;background:#c8d2d6;transition:background 180ms ease; }
  .toggle i::after { content:"";position:absolute;top:2px;left:2px;width:18px;height:18px;border-radius:50%;background:white;box-shadow:0 1px 4px rgba(0,0,0,.2);transition:transform 180ms cubic-bezier(.2,.8,.2,1); }
  .toggle input:checked + i { background:#2aa5d6; } .toggle input:checked + i::after { transform:translateX(16px); }
  .hint { margin:6px 12px 0;color:#89979c;font-size:9px; }
  footer { display:flex;align-items:center;justify-content:flex-end;padding:0 22px;border-top:1px solid rgba(67,91,100,.1);background:rgba(255,255,255,.45); }
  .primary-button { display:inline-flex;align-items:center;gap:7px;height:38px;padding:0 18px;border-radius:999px;font-size:12px;transition:transform 140ms ease,box-shadow 140ms ease; }
  .primary-button:active { transform:scale(.96); }
  :global(.spin) { animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
