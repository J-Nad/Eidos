<script lang="ts">
  import { createEventDispatcher, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Check, File, Folder, Image, LoaderCircle, Save } from 'lucide-svelte';
  import EditorPane from '$lib/EditorPane.svelte';
  import FileCard from '$lib/FileCard.svelte';
  import type { FileMetadata, FolderEntry, PreviewKind } from '$lib/types';

  export let path = '';
  export let exists = false;
  export let isDirectory = false;
  export let content = '';
  export let saving = false;

  const dispatch = createEventDispatcher<{
    contentChange: string;
    loaded: { metadata: FileMetadata | null; content: string };
    save: void;
    error: string;
  }>();

  let editorPane: EditorPane;
  let metadata: FileMetadata | null = null;
  let kind: PreviewKind = 'missing';
  let dataUrl = '';
  let folderEntries: FolderEntry[] = [];
  let loading = false;
  let lastPath = '';

  const textExtensions = new Set(['txt', 'md', 'csv', 'json', 'xml', 'py', 'js', 'ts', 'tsx', 'jsx', 'rs', 'go', 'log', 'yaml', 'yml', 'toml', 'html', 'css', 'sql', 'ps1', 'bat']);
  const imageExtensions = new Set(['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp']);
  const audioExtensions = new Set(['mp3', 'wav', 'ogg', 'flac']);
  const videoExtensions = new Set(['mp4', 'webm', 'mov', 'm4v']);
  const languageByExtension: Record<string, string> = {
    py: 'python', js: 'javascript', mjs: 'javascript', cjs: 'javascript', ts: 'typescript', tsx: 'typescript', jsx: 'javascript',
    rs: 'rust', go: 'go', json: 'json', md: 'markdown', html: 'html', css: 'css', sql: 'sql', xml: 'xml', yaml: 'yaml', yml: 'yaml',
    csv: 'plaintext', txt: 'plaintext', log: 'plaintext', toml: 'ini', ps1: 'powershell', bat: 'bat'
  };

  function classify(extension: string): PreviewKind {
    if (!exists) return 'missing';
    if (isDirectory) return 'folder';
    if (textExtensions.has(extension)) return 'text';
    if (imageExtensions.has(extension)) return 'image';
    if (audioExtensions.has(extension)) return 'audio';
    if (videoExtensions.has(extension)) return 'video';
    if (extension === 'pdf') return 'pdf';
    return 'binary';
  }

  async function load() {
    if (!path || path === lastPath) return;
    lastPath = path;
    loading = true;
    dataUrl = '';
    folderEntries = [];
    metadata = null;
    try {
      if (exists) metadata = await invoke<FileMetadata>('get_file_metadata', { path });
      const extension = metadata?.extension || path.split('.').pop()?.toLowerCase() || 'txt';
      kind = classify(extension);
      if (kind === 'text') {
        content = await invoke<string>('read_file_content', { path });
      } else if (kind === 'image' || kind === 'pdf' || kind === 'audio' || kind === 'video') {
        dataUrl = await invoke<string>('file_data_url', { path });
        content = '';
      } else if (kind === 'folder') {
        folderEntries = await invoke<FolderEntry[]>('list_folder', { path });
        content = '';
      } else if (kind === 'missing') {
        content = '';
      }
      dispatch('loaded', { metadata, content });
    } catch (error) {
      kind = 'binary';
      dispatch('error', String(error));
    } finally {
      loading = false;
    }
  }

  export async function appendToken(token: string) {
    if (kind !== 'text' && kind !== 'missing') kind = 'text';
    content += token;
    dispatch('contentChange', content);
    await tick();
    editorPane?.appendToken(token);
  }

  $: if (path && path !== lastPath) void load();
</script>

<section class="preview">
  <header>
    <div class="file-title">
      {#if kind === 'folder'}<Folder size={15} />{:else if kind === 'image'}<Image size={15} />{:else}<Check size={15} />{/if}
      <strong>{metadata?.filename || path.split(/[\\/]/).pop() || 'Untitled'}</strong>
      <span>{kind}</span>
    </div>
    {#if kind === 'text' || kind === 'missing'}
      <button class="primary-button commit" on:click={() => dispatch('save')} disabled={saving || !content}>
        {#if saving}<LoaderCircle class="spin" size={14} />{:else}<Save size={14} />{/if}
        {saving ? 'Saving…' : 'Save changes'}
      </button>
    {/if}
  </header>

  {#if loading}
    <div class="loading"><LoaderCircle class="spin" size={22} /> Loading preview…</div>
  {:else if kind === 'text' || kind === 'missing'}
    <EditorPane bind:this={editorPane} {content} language={languageByExtension[metadata?.extension || path.split('.').pop()?.toLowerCase() || 'txt'] || 'plaintext'} filename={metadata?.filename || path.split(/[\\/]/).pop() || 'new file'} {saving} showHeader={false} on:change={(event) => { content = event.detail; dispatch('contentChange', content); }} on:commit={() => dispatch('save')} />
  {:else if kind === 'image'}
    <div class="image-wrap"><img alt={metadata?.filename || 'Preview'} src={dataUrl} /></div>
  {:else if kind === 'pdf'}
    <iframe title="PDF preview" src={dataUrl}></iframe>
  {:else if kind === 'audio'}
    <div class="media-wrap"><audio controls src={dataUrl}></audio></div>
  {:else if kind === 'video'}
    <!-- svelte-ignore a11y_media_has_caption -->
    <div class="media-wrap"><video controls src={dataUrl}></video></div>
  {:else if kind === 'folder'}
    <div class="folder-list">
      {#each folderEntries as entry (entry.path)}
        <button on:dblclick={() => invoke('open_file_or_folder', { path: entry.path })}>
          <span>{#if entry.isDirectory}<Folder size={16} />{:else}<File size={16} />{/if}</span>
          <strong>{entry.filename}</strong>
          <small>{entry.isDirectory ? 'Folder' : `${entry.extension || 'file'} · ${entry.size.toLocaleString()} bytes`}</small>
        </button>
      {/each}
    </div>
  {:else}
    <FileCard {metadata} {path} missing={!exists} />
  {/if}
</section>

<style>
  .preview { min-width: 0; height: 100%; display: grid; grid-template-rows: 64px 1fr; overflow: hidden; border: 1px solid rgba(52,63,77,.1); border-radius: 18px; background: rgba(255,255,255,.9); box-shadow: 0 18px 55px rgba(42,52,65,.08); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 0 16px 0 18px; border-bottom: 1px solid rgba(28,39,57,.07); background: rgba(250,251,253,.75); }
  .file-title { min-width: 0; display: flex; align-items: center; gap: 8px; }
  .file-title strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .file-title span { padding: 3px 6px; border-radius: 6px; color: #7c8799; background: rgba(100,114,138,.09); font-size: 8px; text-transform: uppercase; }
  .commit { display: flex; align-items: center; gap: 7px; padding: 8px 14px; border-radius: 9px; font-size: 10px; box-shadow: 0 7px 18px rgba(0,122,255,.18); }
  .loading, .image-wrap, .media-wrap { min-height: 0; display: grid; place-items: center; color: #7e8797; }
  .image-wrap, .media-wrap { padding: 20px; background: #f5f7fb; }
  img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: 12px; box-shadow: 0 14px 50px rgba(22,32,52,.14); }
  audio { width: min(520px, 80%); }
  video { max-width: 100%; max-height: 100%; border-radius: 12px; background: #000; }
  iframe { width: 100%; height: 100%; border: 0; background: #f5f7fb; }
  .folder-list { overflow: auto; padding: 14px; }
  .folder-list button { width: 100%; display: grid; grid-template-columns: 28px 1fr auto; align-items: center; gap: 8px; padding: 10px 12px; border: 0; border-radius: 10px; color: inherit; background: transparent; text-align: left; cursor: default; }
  .folder-list button:hover { background: rgba(0,122,255,.08); }
  .folder-list strong { font-size: 12px; }
  .folder-list small { color: #8d96a6; font-size: 10px; }
  :global(.spin) { animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
