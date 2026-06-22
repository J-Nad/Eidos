<script lang="ts">
  import { get } from 'svelte/store';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification';
  import { ArrowLeft, ExternalLink, LoaderCircle, RotateCcw, Search, Sparkles } from 'lucide-svelte';
  import { toast } from 'svelte-french-toast';
  import PreviewPane from '$lib/PreviewPane.svelte';
  import ChatPane from '$lib/ChatPane.svelte';
  import ToastProvider from '$lib/ToastProvider.svelte';
  import { messageId, splitViewState } from '$lib/stores/studio';
  import type { AgentMode, AiStreamEndPayload, ContextFile, FileMetadata, OpenSplitViewPayload } from '$lib/types';

  let previewPane: PreviewPane;
  let saving = false;
  let indexStatus = '';
  let agentMode: AgentMode = 'agent';
  let contextFiles: Array<ContextFile & { content: string }> = [];
  let undoContent: string | null = null;

  async function applySplitView(payload: OpenSplitViewPayload) {
    contextFiles = [];
    undoContent = null;
    splitViewState.set({
      ...payload,
      metadata: null,
      content: '',
      isGenerating: false,
      activeStreamId: null,
      chatMessages: [{
        id: messageId(),
        role: 'system',
        content: payload.exists
          ? `Loaded file: ${payload.path.split(/[\\/]/).pop() || payload.path}`
          : `File not found: ${payload.path}`
      }]
    });
  }

  async function sendMessage(message: string) {
    const state = get(splitViewState);
    if (!state.aiAvailable || state.isGenerating) return;
    splitViewState.update((value) => ({
      ...value,
      isGenerating: true,
      activeStreamId: null,
      chatMessages: [
        ...value.chatMessages,
        { id: messageId(), role: 'user', content: message },
        { id: messageId(), role: 'assistant', content: '' }
      ]
    }));
    try {
      await invoke('ai_chat_message', {
        filePath: state.exists && !state.isDirectory ? state.path : null,
        message,
        mode: agentMode,
        context: contextFiles.map((file) => `FILE: ${file.path}\n${file.content}`).join('\n\n---\n\n') || null
      });
    } catch (error) {
      splitViewState.update((value) => ({
        ...value,
        isGenerating: false,
        activeStreamId: null,
        chatMessages: [...value.chatMessages, { id: messageId(), role: 'assistant', content: `AI stopped with an error: ${String(error)}` }]
      }));
      toast.error(String(error));
    }
  }

  async function stopGeneration() {
    const streamId = get(splitViewState).activeStreamId;
    await invoke('abort_ai', { streamId });
  }

  async function saveChanges() {
    const state = get(splitViewState);
    if (!state.content || saving) return;
    saving = true;
    const id = toast.loading('Saving atomically…');
    try {
      await invoke('write_file', { path: state.path, content: state.content });
      splitViewState.update((value) => ({ ...value, exists: true }));
      toast.success('Saved changes.', { id });
      let permission = await isPermissionGranted();
      if (!permission) permission = (await requestPermission()) === 'granted';
      if (permission) sendNotification({ title: 'Eidos', body: `${state.path.split(/[\\/]/).pop()} was saved.` });
    } catch (error) {
      toast.error(String(error), { id });
    } finally {
      saving = false;
    }
  }

  async function openDefault() {
    const state = get(splitViewState);
    if (state.exists) await invoke('open_file', { path: state.path });
  }

  function extractApplicableContent(message: string) {
    const tagged = message.match(/<eidos_file>\s*([\s\S]*?)\s*<\/eidos_file>/i);
    if (tagged) return tagged[1].trimEnd();
    const fenced = message.match(/```[A-Za-z0-9_-]*\s*([\s\S]*?)```/);
    return (fenced?.[1] ?? '').trimEnd();
  }

  function applyAssistantContent(message: string) {
    const content = extractApplicableContent(message);
    if (!content) return;
    undoContent = get(splitViewState).content;
    splitViewState.update((value) => ({ ...value, content }));
    toast.success('Proposed edit applied. Review it before saving.');
  }

  function undoAppliedEdit() {
    if (undoContent === null) return;
    const previous = undoContent;
    undoContent = null;
    splitViewState.update((value) => ({ ...value, content: previous }));
  }

  async function attachContext(path: string) {
    if (!path || contextFiles.some((file) => file.path.toLowerCase() === path.toLowerCase())) return;
    try {
      const content = await invoke<string>('read_file_content', { path });
      contextFiles = [...contextFiles, { path, filename: path.split(/[\\/]/).pop() || path, content: content.slice(0, 40_000) }].slice(-5);
    } catch (error) {
      toast.error(`Could not attach context: ${String(error)}`);
    }
  }

  onMount(() => {
    const cleanups: Array<() => void> = [];
    const blockContextMenu = (event: MouseEvent) => event.preventDefault();
    const blockDevtools = (event: KeyboardEvent) => {
      if (event.key === 'F12' || (event.ctrlKey && event.shiftKey && ['I', 'J', 'C'].includes(event.key.toUpperCase()))) event.preventDefault();
    };
    document.documentElement.classList.remove('dark');
    document.addEventListener('contextmenu', blockContextMenu);
    document.addEventListener('keydown', blockDevtools);

    void (async () => {
      cleanups.push(await listen<OpenSplitViewPayload>('open-split-view', ({ payload }) => void applySplitView(payload)));
      cleanups.push(await listen<{ streamId: string }>('ai-stream-start', ({ payload }) => {
        splitViewState.update((value) => ({ ...value, isGenerating: true, activeStreamId: payload.streamId }));
      }));
      cleanups.push(await listen<string>('ai-token', ({ payload }) => {
        splitViewState.update((value) => {
          const messages = [...value.chatMessages];
          const last = messages[messages.length - 1];
          if (last?.role === 'assistant') {
            messages[messages.length - 1] = { ...last, content: last.content + payload };
          } else {
            messages.push({ id: messageId(), role: 'assistant', content: payload });
          }
          return { ...value, chatMessages: messages };
        });
      }));
      cleanups.push(await listen<AiStreamEndPayload>('ai-stream-end', ({ payload }) => {
        splitViewState.update((value) => ({
          ...value,
          isGenerating: false,
          activeStreamId: null,
          chatMessages: payload.cancelled
            ? [...value.chatMessages, { id: messageId(), role: 'assistant', content: 'Stopped.' }]
            : value.chatMessages
        }));
      }));
      cleanups.push(await listen<{ currentDir: string; filesProcessed: number }>('index-progress', ({ payload }) => {
        indexStatus = `${payload.filesProcessed.toLocaleString()} indexed · ${payload.currentDir}`;
      }));
      const pending = await invoke<OpenSplitViewPayload | null>('get_pending_split_view');
      if (pending) await applySplitView(pending);
    })().catch((error) => toast.error(String(error)));

    return () => {
      cleanups.forEach((cleanup) => cleanup());
      document.removeEventListener('contextmenu', blockContextMenu);
      document.removeEventListener('keydown', blockDevtools);
    };
  });
</script>

<div class="split-shell" class:no-ai={!$splitViewState.aiAvailable}>
  <header class="topbar">
    <button class="nav-button" on:click={() => invoke('return_to_spotlight')} title="Back to Eidos"><ArrowLeft size={16} /></button>
    <div class="document-title">
      <strong>{$splitViewState.path.split(/[\\/]/).pop() || 'Eidos'}</strong>
      <span>{$splitViewState.path || 'Open a result from Eidos'}</span>
    </div>
    <span class="agent-status"><Sparkles size={12} /> {indexStatus || ($splitViewState.aiAvailable ? 'Eidos Agent' : 'AI unavailable')}</span>
    {#if $splitViewState.isGenerating}<LoaderCircle class="spin" size={15} />{/if}
    {#if undoContent !== null}<button class="toolbar-button" on:click={undoAppliedEdit}><RotateCcw size={14} /> Undo AI edit</button>{/if}
    {#if $splitViewState.exists}<button class="nav-button" on:click={openDefault} title="Open with default app"><ExternalLink size={15} /></button>{/if}
    <button class="nav-button" on:click={() => invoke('return_to_spotlight')} title="Search"><Search size={15} /></button>
  </header>

  <main>
    {#if $splitViewState.aiAvailable}
      <ChatPane
        messages={$splitViewState.chatMessages}
        generating={$splitViewState.isGenerating}
        disabled={!$splitViewState.path}
        mode={agentMode}
        contextFiles={contextFiles}
        on:send={(event) => sendMessage(event.detail)}
        on:stop={stopGeneration}
        on:apply={(event) => applyAssistantContent(event.detail)}
        on:mode={(event) => agentMode = event.detail}
        on:attach={(event) => attachContext(event.detail)}
        on:removeContext={(event) => contextFiles = contextFiles.filter((file) => file.path !== event.detail)}
      />
    {:else}
      <section class="ai-unavailable">
        <div>
          <strong>AI features not available.</strong>
          <p>Set <code>GEMINI_API_KEY</code> to enable Conductor.</p>
        </div>
      </section>
    {/if}
    <PreviewPane
      bind:this={previewPane}
      path={$splitViewState.path}
      exists={$splitViewState.exists}
      isDirectory={$splitViewState.isDirectory}
      content={$splitViewState.content}
      {saving}
      on:contentChange={(event) => splitViewState.update((value) => ({ ...value, content: event.detail }))}
      on:loaded={(event) => splitViewState.update((value) => ({ ...value, metadata: event.detail.metadata as FileMetadata | null, content: event.detail.content }))}
      on:save={saveChanges}
      on:error={(event) => toast.error(event.detail)}
    />
  </main>
  <ToastProvider />
</div>

<style>
  .split-shell { width: 100vw; height: 100vh; display: grid; grid-template-rows: 64px 1fr; overflow: hidden; color: #26303e; background: linear-gradient(145deg,#f3f5f8,#e9edf2); font-family: "SF Pro Text", "Segoe UI Variable", "Segoe UI", system-ui, sans-serif; }
  .topbar { display: flex; align-items: center; gap: 10px; padding: 0 14px; border-bottom: 1px solid rgba(42,53,68,.09); background: rgba(249,250,252,.76); backdrop-filter: blur(24px) saturate(150%); }
  .document-title { min-width: 0; flex: 1; display: flex; flex-direction: column; }
  .topbar strong { overflow: hidden; font-size: 12px; font-weight: 650; letter-spacing: -.01em; text-overflow: ellipsis; white-space: nowrap; }
  .topbar .document-title span { overflow: hidden; margin-top: 2px; color: #8993a1; font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }
  .nav-button { width: 34px; height: 34px; display: grid; place-items: center; border: 1px solid rgba(52,63,77,.09); border-radius: 10px; color: #667181; background: rgba(255,255,255,.66); cursor: pointer; box-shadow: 0 4px 14px rgba(45,55,68,.05); transition: transform 120ms ease, background 120ms ease; }
  .nav-button:hover { background: #fff; transform: translateY(-1px); }
  .nav-button:active { transform: scale(.94); }
  .agent-status { display: inline-flex; align-items: center; gap: 5px; max-width: 260px; padding: 6px 9px; overflow: hidden; border: 1px solid rgba(44,111,214,.1); border-radius: 999px; color: #55708f; background: rgba(225,238,255,.72); font-size: 9px; text-overflow: ellipsis; white-space: nowrap; }
  .toolbar-button { height: 30px; display: inline-flex; align-items: center; gap: 6px; padding: 0 10px; border: 1px solid rgba(52,63,77,.09); border-radius: 9px; color: #5c6878; background: rgba(255,255,255,.7); cursor: pointer; font-size: 9px; }
  main { min-height: 0; display: grid; grid-template-columns: minmax(360px, .9fr) minmax(460px, 1.1fr); gap: 10px; padding: 10px; }
  .ai-unavailable { display: grid; place-items: center; padding: 24px; border: 1px solid rgba(52,63,77,.1); border-radius: 18px; background: rgba(255,255,255,.76); color: #657084; text-align: center; }
  .ai-unavailable strong { display: block; margin-bottom: 6px; color: #2f3848; font-size: 14px; }
  .ai-unavailable p { margin: 0; font-size: 12px; }
  .ai-unavailable code { padding: 2px 5px; border-radius: 5px; background: rgba(102,116,140,.1); font-family: "Cascadia Code", monospace; }
  :global(.spin) { color: #007aff; animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
