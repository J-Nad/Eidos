<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { ChevronRight, File, Folder, FolderOpen, HardDrive } from 'lucide-svelte';
  import type { FileTreeNode } from '$lib/types';

  export let node: FileTreeNode;
  export let depth = 0;

  const dispatch = createEventDispatcher<{ select: FileTreeNode }>();
  let expanded = depth === 0;

  function activate() {
    if (node.kind === 'file') dispatch('select', node);
    else expanded = !expanded;
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      activate();
    }
  }

  function dragstart(event: DragEvent) {
    if (node.kind !== 'file' || !node.path) return;
    event.dataTransfer?.setData('application/x-eidos-path', node.path);
    event.dataTransfer?.setData('text/plain', node.path);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
  }
</script>

<div
  class:root={node.kind === 'root'}
  class="node"
  style:padding-left={`${10 + depth * 15}px`}
  role="button"
  tabindex="0"
  draggable={node.kind === 'file'}
  on:click={activate}
  on:keydown={keydown}
  on:dragstart={dragstart}
  title={node.path || node.name}
>
  {#if node.kind !== 'file'}
    <ChevronRight class={expanded ? 'expanded' : ''} size={13} strokeWidth={1.8} />
  {:else}
    <span class="spacer"></span>
  {/if}

  {#if node.kind === 'root' && node.source === 'local'}
    <HardDrive size={15} strokeWidth={1.5} />
  {:else if node.kind === 'folder' && expanded}
    <FolderOpen size={15} strokeWidth={1.5} />
  {:else if node.kind === 'folder'}
    <Folder size={15} strokeWidth={1.5} />
  {:else}
    <File size={14} strokeWidth={1.5} />
  {/if}
  <span class="label">{node.name}</span>
</div>

{#if expanded && node.children.length}
  {#each node.children as child (child.id)}
    <svelte:self node={child} depth={depth + 1} on:select={(event) => dispatch('select', event.detail)} />
  {/each}
{/if}

<style>
  .node { display: flex; align-items: center; gap: 7px; height: 30px; margin: 1px 6px; padding-right: 8px; border-radius: 8px; color: #5f6879; cursor: default; outline: none; }
  .node:hover, .node:focus-visible { color: #172033; background: rgba(90, 105, 130, .1); }
  .node.root { height: 34px; margin-top: 6px; color: #2f3848; font-weight: 600; }
  .label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .spacer { width: 13px; flex: 0 0 13px; }
  :global(.node > svg:first-child) { transition: transform 140ms ease; flex: 0 0 auto; }
  :global(.node > svg:first-child.expanded) { transform: rotate(90deg); }
  :global(.dark .node) { color: #929cad; }
  :global(.dark .node:hover), :global(.dark .node:focus-visible) { color: #edf2fa; background: rgba(255,255,255,.07); }
  :global(.dark .node.root) { color: #c5cbd7; }
</style>
