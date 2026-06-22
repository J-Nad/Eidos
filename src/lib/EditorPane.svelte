<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import loader from '@monaco-editor/loader';
  import type { editor as MonacoEditor } from 'monaco-editor';
  import { Check, Code2, LoaderCircle, Save } from 'lucide-svelte';

  export let content = '';
  export let language = 'plaintext';
  export let filename = 'untitled.txt';
  export let saving = false;
  export let showHeader = true;

  const dispatch = createEventDispatcher<{ change: string; commit: void }>();
  let host: HTMLDivElement;
  let editor: MonacoEditor.IStandaloneCodeEditor | undefined;
  let monaco: Awaited<ReturnType<typeof loader.init>> | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let internalUpdate = false;

  loader.config({ paths: { vs: '/monaco/vs' } });

  export function appendToken(token: string) {
    if (!editor || !monaco) return;
    const model = editor.getModel();
    if (!model) return;
    internalUpdate = true;
    const line = model.getLineCount();
    const column = model.getLineMaxColumn(line);
    editor.executeEdits('eidos-stream', [{ range: new monaco.Range(line, column, line, column), text: token }]);
    editor.revealPosition({ lineNumber: model.getLineCount(), column: model.getLineMaxColumn(model.getLineCount()) });
    internalUpdate = false;
  }

  $: if (editor && editor.getValue() !== content && !internalUpdate) {
    internalUpdate = true;
    editor.setValue(content);
    internalUpdate = false;
  }

  $: if (editor && monaco) {
    const model = editor.getModel();
    if (model) monaco.editor.setModelLanguage(model, language);
  }

  onMount(() => {
    let disposed = false;
    void loader.init().then((api) => {
      if (disposed) return;
      monaco = api;
      const dark = document.documentElement.classList.contains('dark');
      editor = api.editor.create(host, {
        value: content,
        language,
        theme: dark ? 'vs-dark' : 'vs',
        automaticLayout: false,
        fontFamily: 'JetBrains Mono, Cascadia Code, monospace',
        fontSize: 13,
        lineHeight: 21,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        smoothScrolling: true,
        padding: { top: 18, bottom: 80 },
        renderLineHighlight: 'gutter',
        overviewRulerBorder: false,
        hideCursorInOverviewRuler: true,
        wordWrap: 'on'
      });
      editor.onDidChangeModelContent(() => {
        if (!internalUpdate) dispatch('change', editor?.getValue() || '');
      });
      resizeObserver = new ResizeObserver(() => editor?.layout());
      resizeObserver.observe(host);
    });
    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      editor?.dispose();
    };
  });
</script>

<section class="editor-pane" class:compact={!showHeader}>
  {#if showHeader}<header>
    <div class="file-title"><Code2 size={15} strokeWidth={1.5} /><strong>{filename}</strong><span>{language}</span></div>
    <button class="primary-button commit" on:click={() => dispatch('commit')} disabled={saving || !content}>
      {#if saving}<LoaderCircle class="spin" size={14} />{:else if content}<Save size={14} />{:else}<Check size={14} />{/if}
      {saving ? 'Committing…' : 'Commit to Disk'}
    </button>
  </header>{/if}
  <div class="editor" bind:this={host}></div>
</section>

<style>
  .editor-pane { min-width: 0; height: 100%; display: grid; grid-template-rows: 58px 1fr; border-right: 1px solid rgba(0,0,0,.05); background: #fff; }
  .editor-pane.compact { grid-template-rows: 1fr; border: 0; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 0 14px 0 18px; border-bottom: 1px solid rgba(28,39,57,.07); }
  .file-title { min-width: 0; display: flex; align-items: center; gap: 8px; }
  .file-title strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .file-title span { padding: 3px 6px; border-radius: 6px; color: #7c8799; background: rgba(100,114,138,.09); font-size: 8px; text-transform: uppercase; }
  .commit { display: flex; align-items: center; gap: 7px; padding: 8px 15px; font-size: 10px; }
  .editor { min-height: 0; width: 100%; }
  :global(.spin) { animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  :global(.dark) .editor-pane { color: #e8ebf1; border-color: rgba(255,255,255,.07); background: #15181e; }
  :global(.dark) header { border-color: rgba(255,255,255,.07); }
</style>
