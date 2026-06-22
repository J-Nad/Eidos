<script lang="ts">
  import { onMount } from 'svelte';
  import SpotlightBar from '$lib/SpotlightBar.svelte';

  onMount(() => {
    const blockContextMenu = (event: MouseEvent) => event.preventDefault();
    const blockDevtools = (event: KeyboardEvent) => {
      if (event.key === 'F12' || (event.ctrlKey && event.shiftKey && ['I', 'J', 'C'].includes(event.key.toUpperCase()))) event.preventDefault();
    };
    document.documentElement.classList.remove('dark');
    document.addEventListener('contextmenu', blockContextMenu);
    document.addEventListener('keydown', blockDevtools);
    return () => {
      document.removeEventListener('contextmenu', blockContextMenu);
      document.removeEventListener('keydown', blockDevtools);
    };
  });
</script>

<main><SpotlightBar /></main>

<style>
  main { position: fixed; inset: 0; display: flex; justify-content: center; align-items: flex-start; pointer-events: none; }
  :global(html), :global(body) { background: transparent !important; }
</style>
