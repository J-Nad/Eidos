<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { ChevronLeft, ChevronRight, HardDrive, LoaderCircle, Plus, Sparkles } from 'lucide-svelte';
  import TreeNode from '$lib/TreeNode.svelte';
  import type { FileTreeNode } from '$lib/types';

  export let tree: FileTreeNode[] = [];
  export let driveSyncing = false;

  const dispatch = createEventDispatcher<{
    scanDrives: void;
    newFile: void;
    select: FileTreeNode;
  }>();
  let collapsed = false;
</script>

<aside class:collapsed>
  <header>
    {#if !collapsed}
      <div class="brand"><span><Sparkles size={15} /></span><strong>Eidos</strong></div>
    {/if}
    <button class="icon-button" on:click={() => collapsed = !collapsed} aria-label="Toggle file index">
      {#if collapsed}<ChevronRight size={17} />{:else}<ChevronLeft size={17} />{/if}
    </button>
  </header>

  {#if collapsed}
    <button class="rail-action" on:click={() => dispatch('newFile')} title="New file"><Plus size={17} /></button>
    <button class="rail-action" on:click={() => dispatch('scanDrives')} title="Index fixed drives"><HardDrive size={16} /></button>
  {:else}
    <div class="section-title">
      <span>Local index</span>
      <div>
        <button class="icon-button small" on:click={() => dispatch('newFile')} title="New file"><Plus size={15} /></button>
        <button class="icon-button small" on:click={() => dispatch('scanDrives')} disabled={driveSyncing} title="Rescan fixed drives">
          {#if driveSyncing}<LoaderCircle class="spin" size={14} />{:else}<HardDrive size={15} />{/if}
        </button>
      </div>
    </div>
    <div class="tree">
      {#each tree as node (node.id)}
        <TreeNode {node} on:select={(event) => dispatch('select', event.detail)} />
      {/each}
      {#if tree.length === 0}<p class="empty">Local Windows files will appear here after indexing.</p>{/if}
    </div>
  {/if}
</aside>

<style>
  aside { width: 250px; min-width: 250px; height: 100%; display: flex; flex-direction: column; border-right: 1px solid rgba(28,39,57,.08); background: rgba(246,248,252,.86); transition: width 180ms ease,min-width 180ms ease; }
  aside.collapsed { width: 52px; min-width: 52px; align-items: center; }
  header { height: 58px; min-height: 58px; display: flex; align-items: center; justify-content: space-between; padding: 0 9px 0 14px; }
  .brand { display: flex; align-items: center; gap: 9px; letter-spacing: -.02em; }
  .brand span { width: 27px; height: 27px; display: grid; place-items: center; color: white; border-radius: 9px; background: linear-gradient(135deg,#007aff,#7847ff); box-shadow: 0 5px 15px rgba(55,92,255,.25); }
  .section-title { display: flex; align-items: center; justify-content: space-between; padding: 4px 9px 5px 16px; color: #858e9f; font-size: 10px; font-weight: 600; letter-spacing: .08em; text-transform: uppercase; }
  .section-title > div { display: flex; }
  .small { width: 27px; height: 27px; border-radius: 8px; }
  .tree { flex: 1; overflow: auto; padding-bottom: 10px; }
  .empty { padding: 12px 18px; color: #929baa; font-size: 12px; }
  .rail-action { width: 34px; height: 34px; display: grid; place-items: center; margin-top: 8px; border: 0; border-radius: 10px; color: #667084; background: transparent; cursor: pointer; }
  .rail-action:hover { background: rgba(127,127,127,.12); }
  :global(.spin) { animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(.dark) aside { color: #eef1f7; border-color: rgba(255,255,255,.07); background: rgba(18,21,27,.95); }
</style>
