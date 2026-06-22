<script lang="ts">
  import { afterUpdate, createEventDispatcher } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { Bot, Check, ChevronDown, FileText, Paperclip, Send, Sparkles, Square, X } from 'lucide-svelte';
  import type { AgentMode, ChatMessage, ContextFile } from '$lib/types';

  export let messages: ChatMessage[] = [];
  export let generating = false;
  export let disabled = false;
  export let mode: AgentMode = 'agent';
  export let contextFiles: ContextFile[] = [];

  const dispatch = createEventDispatcher<{ send: string; stop: void; attach: string; apply: string; mode: AgentMode; removeContext: string }>();
  let message = '';
  let scroll: HTMLDivElement;

  function send() {
    const content = message.trim();
    if (!content || disabled || generating) return;
    message = '';
    dispatch('send', content);
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      send();
    }
  }

  function drop(event: DragEvent) {
    event.preventDefault();
    const path = event.dataTransfer?.getData('application/x-eidos-path') || '';
    if (path) dispatch('attach', path);
  }

  async function attachFile() {
    const selected = await open({ multiple: false, directory: false });
    if (typeof selected === 'string') dispatch('attach', selected);
  }

  function canApply(content: string) {
    return /<eidos_file>[\s\S]*<\/eidos_file>|```[A-Za-z0-9_-]*\s*[\s\S]*?```/.test(content);
  }

  const suggestions = ['Build a complete first version', 'Review this file for bugs', 'Refactor without changing behavior'];

  afterUpdate(() => { if (scroll) scroll.scrollTop = scroll.scrollHeight; });
</script>

<section class="chat" aria-label="Conductor chat" on:dragover|preventDefault on:drop={drop}>
  <header>
    <div><span class="avatar"><Sparkles size={16} /></span><div><strong>Eidos Agent</strong><small>{generating ? 'Working on your file…' : 'File-aware coding copilot'}</small></div></div>
    <label class="mode-picker">
      <select value={mode} on:change={(event) => dispatch('mode', (event.currentTarget as HTMLSelectElement).value as AgentMode)} aria-label="AI mode">
        <option value="ask">Ask</option><option value="edit">Edit</option><option value="agent">Agent</option>
      </select>
      <ChevronDown size={12} />
    </label>
  </header>

  <div class="messages" bind:this={scroll}>
    {#if messages.length === 0}
      <div class="welcome">
        <span class="welcome-mark"><Sparkles size={22} strokeWidth={1.5} /></span>
        <h2>Create with Eidos</h2>
        <p>Describe the outcome. Agent mode uses the open file and any context you attach.</p>
        <div class="suggestions">{#each suggestions as suggestion}<button on:click={() => { message = suggestion; }}>{suggestion}</button>{/each}</div>
      </div>
    {/if}
    {#each messages as item (item.id)}
      <article class:user={item.role === 'user'} class:system={item.role === 'system'}>
        {#if item.role !== 'user'}<span class="message-icon">{#if item.role === 'system'}<Paperclip size={13} />{:else}<Bot size={13} />{/if}</span>{/if}
        <div class="bubble-wrap">
          <p class="allow-select">{item.content}</p>
          {#if item.role === 'assistant' && item.content.trim() && !generating && canApply(item.content)}
            <button class="apply" on:click={() => dispatch('apply', item.content)}><Check size={12} /> Apply proposed file</button>
          {/if}
        </div>
      </article>
    {/each}
    {#if generating}<div class="thinking"><i></i><i></i><i></i></div>{/if}
  </div>

  <div class="composer">
    {#if contextFiles.length}
      <div class="context-row">
        {#each contextFiles as file (file.path)}
          <span><FileText size={11} /> {file.filename}<button on:click={() => dispatch('removeContext', file.path)} aria-label={`Remove ${file.filename}`}><X size={10} /></button></span>
        {/each}
      </div>
    {/if}
    <textarea bind:value={message} on:keydown={keydown} rows="1" placeholder={disabled ? 'Open a file to begin' : mode === 'ask' ? 'Ask about this file…' : 'Describe the change you want…'} disabled={disabled || generating}></textarea>
    <button class="attach" on:click={attachFile} disabled={generating} aria-label="Attach context file"><Paperclip size={15} /></button>
    {#if generating}
      <button class="stop" on:click={() => dispatch('stop')} aria-label="Stop generation"><Square size={13} fill="currentColor" /> Stop</button>
    {:else}
      <button class="send" on:click={send} disabled={disabled || !message.trim()} aria-label="Send"><Send size={16} /></button>
    {/if}
    <small>{mode === 'agent' ? 'Agent can propose a complete edit' : mode === 'edit' ? 'Edit returns an applicable file' : 'Ask keeps your file unchanged'} · Enter to send</small>
  </div>
</section>

<style>
  .chat { min-width: 0; height: 100%; display: grid; grid-template-rows: 64px 1fr auto; overflow: hidden; border: 1px solid rgba(52,63,77,.1); border-radius: 18px; background: rgba(255,255,255,.78); box-shadow: 0 18px 55px rgba(42,52,65,.08); backdrop-filter: blur(24px) saturate(150%); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 0 18px; border-bottom: 1px solid rgba(28,39,57,.07); background: rgba(250,251,253,.72); }
  header > div { display: flex; align-items: center; gap: 9px; }
  header strong, header small { display: block; }
  header strong { font-size: 13px; }
  header small { margin-top: 1px; color: #929baa; font-size: 10px; }
  .avatar { width: 32px; height: 32px; display: grid; place-items: center; color: #fff; border-radius: 10px; background: linear-gradient(145deg, #2288ff, #7467f0); box-shadow: 0 6px 16px rgba(58,112,232,.25); }
  .mode-picker { position: relative; height: 30px; display: flex; align-items: center; color: #6f7988; border: 1px solid rgba(45,57,72,.1); border-radius: 9px; background: rgba(255,255,255,.75); }
  .mode-picker select { appearance: none; height: 100%; padding: 0 27px 0 10px; border: 0; outline: 0; color: #3f4958; background: transparent; font-size: 10px; font-weight: 650; }
  .mode-picker :global(svg) { position: absolute; right: 8px; pointer-events: none; }
  .messages { min-height: 0; overflow: auto; padding: 24px 22px 18px; scrollbar-width: thin; }
  .welcome { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; color: #8993a2; text-align: center; }
  .welcome-mark { width: 48px; height: 48px; display: grid; place-items: center; color: #fff; border-radius: 15px; background: linear-gradient(145deg,#278cff,#7769ef); box-shadow: 0 14px 28px rgba(65,107,219,.22); }
  .welcome h2 { margin: 16px 0 6px; color: #303a49; font-size: 18px; font-weight: 650; letter-spacing: -.02em; }
  .welcome p { max-width: 330px; margin: 0; font-size: 11px; line-height: 1.55; }
  .suggestions { width: min(330px, 90%); display: grid; gap: 7px; margin-top: 20px; }
  .suggestions button { padding: 9px 12px; border: 1px solid rgba(45,57,72,.09); border-radius: 10px; color: #5e6878; background: rgba(255,255,255,.7); cursor: pointer; font-size: 10px; text-align: left; transition: transform 120ms ease, background 120ms ease; }
  .suggestions button:hover { background: #fff; transform: translateY(-1px); }
  article { display: flex; align-items: flex-start; gap: 8px; margin: 0 0 17px; }
  .bubble-wrap { max-width: 86%; }
  article p { margin: 0; padding: 11px 13px; border: 1px solid rgba(49,61,76,.07); border-radius: 5px 15px 15px 15px; color: #3e485a; background: rgba(255,255,255,.92); box-shadow: 0 5px 18px rgba(35,47,70,.055); font-size: 12px; line-height: 1.58; white-space: pre-wrap; }
  article.user { justify-content: flex-end; }
  article.user p { color: white; border-radius: 14px 4px 14px 14px; background: linear-gradient(135deg,#1682ff,#5364ed); }
  article.system p { color: #687386; border: 1px dashed rgba(76,93,124,.2); background: rgba(255,255,255,.5); }
  .apply { display: inline-flex; align-items: center; gap: 5px; margin-top: 7px; padding: 6px 10px; border: 1px solid rgba(0,122,255,.12); border-radius: 999px; color: #1268c3; background: rgba(0,122,255,.08); cursor: pointer; font-size: 10px; font-weight: 650; }
  .message-icon { width: 23px; height: 23px; display: grid; place-items: center; flex: 0 0 auto; color: #6674eb; border-radius: 8px; background: rgba(91,105,235,.1); }
  .thinking { display: flex; gap: 4px; padding: 5px 32px; }
  .thinking i { width: 5px; height: 5px; border-radius: 99px; background: #8b94a4; animation: pulse 1s infinite alternate; }
  .thinking i:nth-child(2) { animation-delay: .2s; } .thinking i:nth-child(3) { animation-delay: .4s; }
  .composer { position: relative; margin: 0 16px 16px; padding: 8px 48px 24px 42px; border: 1px solid rgba(47,61,86,.12); border-radius: 16px; background: rgba(255,255,255,.94); box-shadow: 0 12px 35px rgba(31,43,67,.09); }
  textarea { width: 100%; min-height: 39px; max-height: 120px; resize: none; padding: 8px 0 0; border: 0; outline: 0; color: #2c3545; background: transparent; font-family: inherit; font-size: 12px; line-height: 1.5; }
  .composer small { position: absolute; left: 42px; bottom: 7px; color: #9aa4b2; font-size: 8px; }
  .attach { position: absolute; left: 9px; top: 10px; width: 30px; height: 30px; display: grid; place-items: center; border: 0; border-radius: 9px; color: #7b8594; background: rgba(99,113,135,.08); cursor: pointer; }
  .context-row { grid-column: 1 / -1; display: flex; gap: 5px; overflow-x: auto; padding: 0 0 5px; margin-left: -31px; }
  .context-row > span { display: inline-flex; align-items: center; gap: 4px; max-width: 170px; padding: 4px 6px; border-radius: 7px; color: #536172; background: #edf3f9; font-size: 8px; white-space: nowrap; }
  .context-row span button { display: grid; place-items: center; padding: 0; border: 0; color: #7c8797; background: transparent; cursor: pointer; }
  .send { position: absolute; right: 9px; top: 10px; width: 31px; height: 31px; display: grid; place-items: center; border: 0; border-radius: 10px; color: white; background: linear-gradient(135deg,#007aff,#5663ed); cursor: pointer; }
  .send:disabled { opacity: .35; cursor: default; }
  .stop { position: absolute; right: 8px; top: 9px; display: flex; align-items: center; gap: 5px; height: 31px; padding: 0 10px; border: 0; border-radius: 9px; color: #d74855; background: #fff0f1; cursor: pointer; font-size: 10px; font-weight: 600; }
  @keyframes pulse { to { opacity: .25; transform: translateY(-2px); } }
</style>
