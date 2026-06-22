<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let query = '';
  export let filename = '';
  export let extension = 'txt';
  export let creating = false;

  const dispatch = createEventDispatcher<{
    create: { filename: string; extension: string };
    cancel: void;
    extensionChange: string;
    filenameChange: string;
  }>();

  const extensions = ['txt', 'md', 'py', 'js', 'ts', 'json', 'csv', 'log', 'yaml', 'toml', 'rs', 'go', 'html', 'css'];

  $: displayQuery = query.trim() || filename || 'untitled';
</script>

<div class="create-prompt">
  <p>No results. Create a new file named <strong>“{displayQuery}”</strong>?</p>
  <div class="create-controls">
    <input
      value={filename}
      on:input={(event) => dispatch('filenameChange', (event.currentTarget as HTMLInputElement).value)}
      aria-label="File name"
      spellcheck="false"
    />
    <select
      value={extension}
      on:change={(event) => dispatch('extensionChange', (event.currentTarget as HTMLSelectElement).value)}
      aria-label="File type"
    >
      {#each extensions as option}
        <option value={option}>.{option}</option>
      {/each}
    </select>
  </div>
  <div class="actions">
    <button class="cancel" on:click={() => dispatch('cancel')} disabled={creating}>Cancel</button>
    <button class="create" on:click={() => dispatch('create', { filename, extension })} disabled={creating || !filename.trim()}>
      {creating ? 'Creating…' : 'Create'}
    </button>
  </div>
</div>

<style>
  .create-prompt { padding: 14px 14px 13px; border-top: 1px solid rgba(40,51,70,.07); }
  p { margin: 0 0 10px; color: #5b6575; font-size: 12px; line-height: 1.45; }
  strong { color: #1f2938; font-weight: 600; }
  .create-controls { display: grid; grid-template-columns: 1fr 82px; gap: 8px; }
  input, select { height: 34px; border: 1px solid rgba(40,51,70,.11); border-radius: 9px; outline: none; color: inherit; background: rgba(255,255,255,.65); font-size: 12px; }
  input { min-width: 0; padding: 0 10px; }
  select { padding: 0 8px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
  button { height: 30px; padding: 0 13px; border: 0; border-radius: 999px; cursor: pointer; font-size: 11px; font-weight: 600; }
  button:disabled { opacity: .55; cursor: default; }
  .cancel { color: #657084; background: rgba(102,116,140,.1); }
  .create { color: white; background: linear-gradient(110deg,#007aff,#0055ff); box-shadow: 0 7px 18px rgba(0,91,255,.22); }
  :global(.dark) .create-prompt { border-color: rgba(255,255,255,.07); }
  :global(.dark) p { color: #aab3c2; }
  :global(.dark) strong { color: #edf1f7; }
  :global(.dark) input, :global(.dark) select { border-color: rgba(255,255,255,.09); background: rgba(255,255,255,.07); }
</style>
