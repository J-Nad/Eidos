<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { FolderOpen, HardDrive, KeyRound, LoaderCircle, Power, Save, X } from 'lucide-svelte';
  import { toast } from 'svelte-french-toast';
  import type { DriveEntry, Settings } from '$lib/types';

  const dispatch = createEventDispatcher<{
    close: void;
    saved: Settings;
  }>();

  let drives: DriveEntry[] = [];
  let settings: Settings = {
    selectedDrives: [],
    defaultSaveLocation: '',
    autostart: false,
    geminiApiKey: ''
  };
  let loading = true;
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
      dispatch('saved', settings);
    } catch (error) {
      toast.error(String(error), { id });
    } finally {
      saving = false;
    }
  }

  onMount(() => {
    void (async () => {
      const [availableDrives, savedSettings] = await Promise.all([
        invoke<DriveEntry[]>('available_drives'),
        invoke<Settings>('get_settings')
      ]);
      drives = availableDrives;
      settings = savedSettings;
    })()
      .catch((error) => toast.error(String(error)))
      .finally(() => {
        loading = false;
      });
  });
</script>

<div class="settings-card">
  <header>
    <div>
      <strong>Settings</strong>
      <span>Update Eidos without leaving the command bar.</span>
    </div>
    <button class="icon-button" on:click={() => dispatch('close')} aria-label="Close settings">
      <X size={15} strokeWidth={2} />
    </button>
  </header>

  {#if loading}
    <div class="loading">
      <LoaderCircle class="spin" size={18} />
      <span>Loading settings…</span>
    </div>
  {:else}
    <div class="content">
      <section>
        <h2>Search locations</h2>
        <div class="card drive-list">
          {#each drives as drive (drive.path)}
            <label class="setting-row">
              <span class="symbol"><HardDrive size={17} strokeWidth={1.7} /></span>
              <span class="row-copy">
                <strong>{drive.label}</strong>
                <small>Include this drive in Eidos results</small>
              </span>
              <input class="check" type="checkbox" checked={settings.selectedDrives.includes(drive.path)} on:change={() => toggleDrive(drive.path)} />
            </label>
          {/each}
        </div>
      </section>

      <section>
        <h2>Files</h2>
        <div class="card field-card">
          <span class="symbol"><FolderOpen size={17} strokeWidth={1.7} /></span>
          <label>
            <span>Default save location</span>
            <input class="text-input" bind:value={settings.defaultSaveLocation} spellcheck="false" placeholder="Example: C:\Users\You\Documents" />
          </label>
        </div>
      </section>

      <section class="two-column">
        <div>
          <h2>General</h2>
          <div class="card compact-card">
            <label class="setting-row compact-row">
              <span class="symbol"><Power size={17} strokeWidth={1.7} /></span>
              <span class="row-copy">
                <strong>Open at login</strong>
                <small>Start Eidos with Windows</small>
              </span>
              <span class="toggle">
                <input type="checkbox" bind:checked={settings.autostart} />
                <i></i>
              </span>
            </label>
          </div>
        </div>

        <div>
          <h2>Intelligence</h2>
          <div class="card field-card compact-field">
            <span class="symbol violet"><KeyRound size={17} strokeWidth={1.7} /></span>
            <label>
              <span>Gemini API key</span>
              <input class="text-input" type="password" bind:value={settings.geminiApiKey} placeholder="Optional · protected by Windows" autocomplete="off" />
            </label>
          </div>
        </div>
      </section>
    </div>

    <footer>
      <button class="ghost" on:click={() => dispatch('close')}>Done</button>
      <button class="primary" on:click={save} disabled={saving}>
        {#if saving}<LoaderCircle class="spin" size={14} />{:else}<Save size={14} />{/if}
        Save
      </button>
    </footer>
  {/if}
</div>

<style>
  .settings-card { width: min(572px, calc(100vw - 22px)); max-height: 430px; display: grid; grid-template-rows: auto minmax(0, 1fr) auto; overflow: hidden; border: 1px solid rgba(83,104,111,.24); border-radius: 18px; color: #34444b; background: rgba(247,251,252,.985); box-shadow: 0 18px 40px rgba(29,42,48,.16); backdrop-filter: blur(30px) saturate(140%); -webkit-backdrop-filter: blur(30px) saturate(140%); animation: drop 140ms ease-out; font-family: "Segoe UI Variable","SF Pro Text","Segoe UI",sans-serif; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 14px 14px 10px; border-bottom: 1px solid rgba(80,110,120,.12); }
  header strong, header span { display: block; }
  header strong { color: #34444b; font-size: 15px; font-weight: 700; letter-spacing: -.02em; }
  header span { margin-top: 2px; color: #71848b; font-size: 11px; font-weight: 500; }
  .icon-button { width: 28px; height: 28px; display: grid; place-items: center; border: 0; border-radius: 50%; color: #657980; background: rgba(91,116,125,.1); cursor: pointer; transition: transform 120ms ease, background 120ms ease; }
  .icon-button:hover { background: rgba(91,116,125,.17); }
  .icon-button:active { transform: scale(.92); }
  .loading { min-height: 180px; display: grid; place-items: center; align-content: center; gap: 8px; color: #657980; font-size: 12px; font-weight: 650; }
  .content { min-height: 0; overflow-y: auto; padding: 12px 14px; scrollbar-width: thin; }
  .content::-webkit-scrollbar { width: 8px; }
  .content::-webkit-scrollbar-thumb { border-radius: 999px; background: rgba(103,116,137,.3); }
  section + section { margin-top: 12px; }
  h2 { margin: 0 0 6px 9px; color: #64747b; font-size: 10px; font-weight: 750; letter-spacing: .04em; text-transform: uppercase; }
  .two-column { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 10px; }
  .card { overflow: hidden; border: 1px solid rgba(80,110,120,.18); border-radius: 15px; background: rgba(255,255,255,.58); box-shadow: inset 0 1px 1px rgba(255,255,255,.7); }
  .drive-list { max-height: 116px; overflow-y: auto; scrollbar-width: thin; }
  .setting-row { min-height: 48px; display: flex; align-items: center; gap: 10px; padding: 7px 10px; color: #34444b; }
  .setting-row + .setting-row { border-top: 1px solid rgba(80,110,120,.1); }
  .compact-row { min-height: 54px; }
  .symbol { width: 30px; height: 30px; display: grid; place-items: center; flex: 0 0 30px; border-radius: 9px; color: #397e9a; background: #dff3fa; }
  .symbol.violet { color: #7259bb; background: #eee9ff; }
  .row-copy { min-width: 0; flex: 1; }
  .row-copy strong, .row-copy small { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-copy strong { font-size: 12px; font-weight: 650; }
  .row-copy small { margin-top: 2px; color: #7d8d93; font-size: 10px; font-weight: 500; }
  .check { width: 17px; height: 17px; flex: 0 0 auto; accent-color: #2b9ed0; }
  .field-card { display: flex; align-items: center; gap: 10px; padding: 10px; }
  .compact-field { min-height: 54px; }
  .field-card label { min-width: 0; flex: 1; display: grid; gap: 5px; color: #52666e; font-size: 10px; font-weight: 700; }
  .text-input { width: 100%; min-width: 0; height: 32px; border: 1px solid rgba(80,110,120,.2); border-radius: 11px; padding: 0 10px; outline: none; color: #2c4149; background: rgba(247,251,252,.82); font: inherit; font-size: 11px; font-weight: 550; transition: border 120ms ease, box-shadow 120ms ease; }
  .text-input:focus { border-color: rgba(0,122,255,.45); box-shadow: 0 0 0 3px rgba(0,122,255,.08); }
  .toggle { position: relative; width: 38px; height: 22px; flex: 0 0 38px; }
  .toggle input { position: absolute; inset: 0; z-index: 1; opacity: 0; cursor: pointer; }
  .toggle i { display: block; width: 38px; height: 22px; border-radius: 999px; background: #c8d2d6; transition: background 160ms ease; }
  .toggle i::after { content: ""; position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: white; box-shadow: 0 1px 4px rgba(0,0,0,.2); transition: transform 160ms cubic-bezier(.2,.8,.2,1); }
  .toggle input:checked + i { background: #2aa5d6; }
  .toggle input:checked + i::after { transform: translateX(16px); }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding: 10px 14px 14px; border-top: 1px solid rgba(80,110,120,.12); }
  button { font-family: inherit; }
  .ghost, .primary { height: 34px; display: inline-flex; align-items: center; gap: 7px; border: 0; border-radius: 999px; padding: 0 14px; font-size: 12px; font-weight: 750; cursor: pointer; transition: transform 120ms ease, background 120ms ease, box-shadow 120ms ease; }
  .ghost { color: #53656c; background: rgba(102,116,140,.12); }
  .primary { color: white; background: linear-gradient(135deg,#007aff,#5b7cff); box-shadow: 0 8px 18px rgba(0,122,255,.2); }
  .primary:disabled { cursor: default; opacity: .65; }
  .ghost:active, .primary:active { transform: scale(.96); }
  :global(.spin) { animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes drop { from { opacity: 0; transform: translateY(-4px) scale(.992); } to { opacity: 1; transform: none; } }
</style>
