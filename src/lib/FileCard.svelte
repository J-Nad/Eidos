<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Box, ExternalLink } from 'lucide-svelte';
  import type { FileMetadata } from '$lib/types';

  export let metadata: FileMetadata | null = null;
  export let path = '';
  export let missing = false;

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
    return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
  };

  async function openDefault() {
    if (path) await invoke('open_file', { path });
  }
</script>

<div class="card">
  <Box size={34} strokeWidth={1.35} />
  <h2>{missing ? 'File not found' : metadata?.filename || 'Unsupported preview'}</h2>
  <p>{missing ? 'Eidos can create this file when Gemini is configured.' : path}</p>
  {#if metadata}
    <dl>
      <div><dt>Type</dt><dd>{metadata.isDirectory ? 'Folder' : metadata.extension || 'File'}</dd></div>
      <div><dt>Size</dt><dd>{metadata.isDirectory ? '—' : formatSize(metadata.size)}</dd></div>
      <div><dt>Modified</dt><dd>{metadata.modifiedTimestamp ? new Date(metadata.modifiedTimestamp * 1000).toLocaleString() : 'Unknown'}</dd></div>
    </dl>
  {/if}
  {#if !missing}
    <button class="primary-button" on:click={openDefault}><ExternalLink size={15} /> Open with default program</button>
  {/if}
</div>

<style>
  .card { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 36px; color: #5f6979; text-align: center; }
  h2 { margin: 0; color: #20293a; font-size: 20px; }
  p { max-width: 560px; margin: 0; overflow-wrap: anywhere; font-size: 12px; line-height: 1.55; }
  dl { width: min(420px, 100%); margin: 8px 0 6px; padding: 13px; border-radius: 14px; background: rgba(102,116,140,.07); text-align: left; }
  dl div { display: flex; justify-content: space-between; gap: 16px; padding: 6px 0; }
  dt { color: #8c95a5; font-size: 10px; text-transform: uppercase; letter-spacing: .06em; }
  dd { margin: 0; font-size: 12px; }
  button { display: inline-flex; align-items: center; gap: 7px; }
  :global(.dark) h2 { color: #edf1f7; }
  :global(.dark) .card { color: #a8b1c1; }
  :global(.dark) dl { background: rgba(255,255,255,.06); }
</style>
