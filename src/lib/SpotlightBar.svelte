<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { LogicalSize, getCurrentWindow } from '@tauri-apps/api/window';
  import { File, FileSearch, Filter, Folder, LoaderCircle, Search, Settings, Sparkles, X } from 'lucide-svelte';
  import { toast } from 'svelte-french-toast';
  import CreationPrompt from '$lib/CreationPrompt.svelte';
  import FilterPanel from '$lib/FilterPanel.svelte';
  import SettingsPanel from '$lib/SettingsPanel.svelte';
  import ToastProvider from '$lib/ToastProvider.svelte';
  import { activeFilters, emptyMetadataFilters, hasActiveFilters, normalizeFilters } from '$lib/stores/metadataFilters';
  import type { FileEntry, IndexStats, MetadataFilters } from '$lib/types';

  const appWindow = getCurrentWindow();
  let input: HTMLInputElement;
  let query = '';
  let results: FileEntry[] = [];
  let contentResults: FileEntry[] = [];
  let searching = false;
  let indexing = false;
  let selected = 0;
  let progress = '';
  let indexedCount = 0;
  let showCreatePrompt = false;
  let creating = false;
  let createFilename = '';
  let createExtension = 'txt';
  let filterOpen = false;
  let settingsOpen = false;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let metadataParseTimer: ReturnType<typeof setTimeout> | undefined;
  let requestVersion = 0;
  let parseRequestVersion = 0;

  $: filters = normalizeFilters($activeFilters);
  $: filtersActive = hasActiveFilters(filters);
  $: selectableResults = [...results, ...contentResults];
  $: filterChips = describeFilters(filters);

  function resetLauncher() {
    clearTimeout(debounceTimer);
    clearTimeout(metadataParseTimer);
    requestVersion += 1;
    parseRequestVersion += 1;
    query = '';
    results = [];
    contentResults = [];
    searching = false;
    selected = 0;
    showCreatePrompt = false;
    creating = false;
    filterOpen = false;
    settingsOpen = false;
    progress = '';
    activeFilters.set({ ...emptyMetadataFilters, extensions: [] });
    void resizeWindow('closed');
    requestAnimationFrame(() => input?.focus());
  }

  const knownExtensions = new Set(['txt', 'md', 'py', 'js', 'ts', 'json', 'csv', 'log', 'yaml', 'toml', 'rs', 'go', 'html', 'css']);

  function inferParts(value: string) {
    const trimmed = value.trim().replace(/[<>:"|?*]/g, '-');
    const leaf = trimmed.split(/[\\/]/).pop() || 'untitled';
    const match = leaf.match(/^(.*)\.([A-Za-z0-9]{1,10})$/);
    if (match) {
      return {
        filename: match[1] || 'untitled',
        extension: knownExtensions.has(match[2].toLowerCase()) ? match[2].toLowerCase() : 'txt'
      };
    }
    return { filename: leaf || 'untitled', extension: 'txt' };
  }

  function buildCreatePath() {
    const base = createFilename.trim() || 'untitled';
    const ext = createExtension.trim().replace(/^\./, '') || 'txt';
    const original = query.trim();
    const directory = original.match(/^(.*[\\/])[^\\/]*$/)?.[1] || '';
    return `${directory}${base}.${ext}`;
  }

  function matchParts(filename: string) {
    const needle = query.trim().toLowerCase();
    const haystack = filename.toLowerCase();
    const index = needle ? haystack.indexOf(needle) : -1;
    if (index < 0) return { before: filename, match: '', after: '' };
    return {
      before: filename.slice(0, index),
      match: filename.slice(index, index + needle.length),
      after: filename.slice(index + needle.length)
    };
  }

  async function resizeWindow(mode: 'closed' | 'results' | 'prompt' | 'filters' | 'settings') {
    const visibleRows = Math.min(selectableResults.length || 1, 5);
    const resultSections = (results.length ? 1 : 0) + (contentResults.length ? 1 : 0) + (filtersActive ? 1 : 0);
    const resultsHeight = Math.min(
      440,
      Math.max(238, 104 + visibleRows * 50 + resultSections * 23 + (indexedCount > 0 ? 22 : 0))
    );
    const height = mode === 'closed'
      ? 82
      : mode === 'filters'
        ? 405
      : mode === 'settings'
        ? 520
      : mode === 'prompt'
        ? 218
        : resultsHeight;
    await appWindow.setSize(new LogicalSize(660, height));
  }

  function prepareCreatePrompt() {
    const parts = inferParts(query);
    createFilename = parts.filename;
    createExtension = parts.extension;
    showCreatePrompt = true;
    void resizeWindow('prompt');
  }

  function scheduleSearch() {
    clearTimeout(debounceTimer);
    clearTimeout(metadataParseTimer);
    showCreatePrompt = false;
    filterOpen = false;
    settingsOpen = false;
    const value = query.trim();
    if (!value && !filtersActive) {
      results = [];
      contentResults = [];
      searching = false;
      void resizeWindow('closed');
      return;
    }
    debounceTimer = setTimeout(() => void search(value), filtersActive ? 0 : 80);
    if (looksLikeMetadataLanguage(value)) {
      metadataParseTimer = setTimeout(() => void parseMetadataLanguage(value), 500);
    }
  }

  function looksLikeMetadataLanguage(value: string) {
    return /(?:\*\.[a-z0-9]+|\b(?:type|kind|ext|extension)\s*[:=]|\bmodified\b|\bcreated\b|\bpast\s+\d+\s+days?\b|\b(?:today|yesterday|last week|this week|this month|last month)\b|\b(?:larger|bigger|smaller|less|size\s*[<>]|over|under)\s+\d|\b(?:pdf|documents?|spreadsheets?|presentations?|photos?|images?|videos?|music|audio|code|archives?|folders?)\b)/i.test(value);
  }

  function hasContentIntent(value: string) {
    return /(?:\bcontaining\b|\bwith\s+the\s+phrase\b|\binside\b|"[^"]+")/i.test(value);
  }

  function withTimeout<T>(promise: Promise<T>, milliseconds: number, message: string): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeoutPromise = new Promise<T>((_, reject) => {
      timer = setTimeout(() => reject(new Error(message)), milliseconds);
    });
    return Promise.race([promise, timeoutPromise]).finally(() => {
      if (timer) clearTimeout(timer);
    });
  }

  async function parseMetadataLanguage(value: string) {
    const version = ++parseRequestVersion;
    try {
      const parsed = await withTimeout(
        invoke<MetadataFilters>('parse_natural_language', { query: value }),
        2000,
        'Metadata interpretation took too long.'
      );
      if (version !== parseRequestVersion || value !== query.trim()) return;
      const normalized = normalizeFilters(parsed);
      const onlyNameFallback =
        !normalized.extensions?.length &&
        !normalized.kind &&
        !normalized.sizeMin &&
        !normalized.sizeMax &&
        !normalized.modifiedAfter &&
        !normalized.modifiedBefore &&
        !normalized.createdAfter &&
        !normalized.createdBefore &&
        !normalized.hasContentIntent &&
        normalized.nameQuery?.toLowerCase() === value.toLowerCase();
      if (onlyNameFallback) return;
      activeFilters.set(normalized);
      await search(value);
    } catch (error) {
      if (version !== parseRequestVersion) return;
      // Filename search remains available when interpretation is unavailable.
    }
  }

  async function search(value: string) {
    const version = ++requestVersion;
    searching = true;
    try {
      const active = hasActiveFilters($activeFilters);
      const found = active
        ? await invoke<FileEntry[]>('search_with_filters', { query: value, filters: normalizeFilters($activeFilters) })
        : await invoke<FileEntry[]>('search_files', { query: value });
      if (version !== requestVersion || value !== query.trim()) return;
      results = found;
      contentResults = [];
      if (hasContentIntent(value) || $activeFilters.hasContentIntent) {
        try {
          const deep = await withTimeout(
            invoke<FileEntry[]>('search_content_phrase', { query: value, filters: normalizeFilters($activeFilters) }),
            1800,
            'Content search timed out.'
          );
          if (version !== requestVersion || value !== query.trim()) return;
          const filenamePaths = new Set(results.map((entry) => entry.path.toLowerCase()));
          contentResults = deep.filter((entry) => !filenamePaths.has(entry.path.toLowerCase()));
        } catch (error) {
          // Content search is optional and must never block filename results.
          contentResults = [];
        }
      }
      selected = 0;
      await resizeWindow('results');
    } catch (error) {
      results = [];
      contentResults = [];
      toast.error('Search is temporarily unavailable.');
    } finally {
      if (version === requestVersion) searching = false;
    }
  }

  async function openResult(entry: FileEntry, splitView: boolean) {
    try {
      if (splitView) {
        await invoke('open_split_view', { path: entry.path });
      } else {
        await invoke('open_file', { path: entry.path });
      }
      await appWindow.hide();
    } catch (error) {
      toast.error(String(error) || 'Could not open this item.');
    }
  }

  async function createFile() {
    if (creating) return;
    creating = true;
    try {
      await invoke('create_file', { path: buildCreatePath(), extension: createExtension, content: null });
      await appWindow.hide();
    } catch (error) {
      toast.error(String(error) || 'Could not create the file.');
    } finally {
      creating = false;
    }
  }

  function clickResult(event: MouseEvent, entry: FileEntry) {
    const splitView = event.ctrlKey || event.button === 1;
    void openResult(entry, splitView);
  }

  function toggleSettings() {
    settingsOpen = !settingsOpen;
    filterOpen = false;
    showCreatePrompt = false;
    void resizeWindow(settingsOpen ? 'settings' : (query.trim() || filtersActive ? 'results' : 'closed'));
  }

  function toggleFilters() {
    filterOpen = !filterOpen;
    settingsOpen = false;
    showCreatePrompt = false;
    void resizeWindow(filterOpen ? 'filters' : (query.trim() || filtersActive ? 'results' : 'closed'));
  }

  function applyFilters(event: CustomEvent<MetadataFilters>) {
    activeFilters.set(normalizeFilters(event.detail));
    filterOpen = false;
    settingsOpen = false;
    void search(query.trim());
  }

  function clearFilters() {
    activeFilters.set({ ...emptyMetadataFilters, extensions: [] });
    filterOpen = false;
    settingsOpen = false;
    void search(query.trim());
  }

  function closeSettings() {
    settingsOpen = false;
    void resizeWindow(query.trim() || filtersActive ? 'results' : 'closed');
  }

  function removeFilter(kind: string) {
    const next = normalizeFilters($activeFilters);
    if (kind === 'extension') next.extensions = [];
    if (kind === 'kind') next.kind = null;
    if (kind === 'size') {
      next.sizeMin = null;
      next.sizeMax = null;
    }
    if (kind === 'modified') {
      next.modifiedAfter = null;
      next.modifiedBefore = null;
    }
    if (kind === 'created') {
      next.createdAfter = null;
      next.createdBefore = null;
    }
    if (kind === 'name') next.nameQuery = null;
    activeFilters.set(next);
    void search(query.trim());
  }

  function describeDateRange(after?: number | null, before?: number | null) {
    const format = (timestamp: number) => new Date(timestamp * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
    if (after && before) return `${format(after)}-${format(before)}`;
    if (after) return `after ${format(after)}`;
    if (before) return `before ${format(before)}`;
    return '';
  }

  function describeFilters(value: MetadataFilters) {
    const chips: { kind: string; label: string }[] = [];
    if (value.extensions?.length) chips.push({ kind: 'extension', label: value.extensions.map((item) => `.${item}`).join(', ') });
    if (value.kind) chips.push({ kind: 'kind', label: value.kind });
    if (value.sizeMin || value.sizeMax) {
      const mb = (bytes: number) => `${Math.round((bytes / 1024 / 1024) * 10) / 10}MB`;
      chips.push({ kind: 'size', label: `${value.sizeMin ? `>${mb(value.sizeMin)}` : ''}${value.sizeMin && value.sizeMax ? ' ' : ''}${value.sizeMax ? `<${mb(value.sizeMax)}` : ''}` });
    }
    if (value.modifiedAfter || value.modifiedBefore) chips.push({ kind: 'modified', label: `modified ${describeDateRange(value.modifiedAfter, value.modifiedBefore)}` });
    if (value.createdAfter || value.createdBefore) chips.push({ kind: 'created', label: `created ${describeDateRange(value.createdAfter, value.createdBefore)}` });
    if (value.nameQuery) chips.push({ kind: 'name', label: `name ${value.nameQuery}` });
    return chips;
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'ArrowDown' && !showCreatePrompt) {
      event.preventDefault();
      selected = Math.min(selected + 1, Math.max(0, selectableResults.length - 1));
    } else if (event.key === 'ArrowUp' && !showCreatePrompt) {
      event.preventDefault();
      selected = Math.max(0, selected - 1);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (showCreatePrompt) void createFile();
      else if (filterOpen) {
        filterOpen = false;
        void search(query.trim());
      }
      else if (settingsOpen) closeSettings();
      else if (selectableResults[selected]) void openResult(selectableResults[selected], event.ctrlKey);
      else if (!searching && !indexing && !filtersActive) prepareCreatePrompt();
    } else if (event.key === 'Escape') {
      if (filterOpen) {
        filterOpen = false;
        void resizeWindow(query.trim() || filtersActive ? 'results' : 'closed');
      } else if (settingsOpen) {
        closeSettings();
      } else if (showCreatePrompt) {
        showCreatePrompt = false;
        void resizeWindow(query.trim() ? 'results' : 'closed');
      } else {
        void appWindow.hide();
      }
    }
  }

  onMount(() => {
    input.focus();
    const cleanupProgress = listen<{ currentDir: string; filesProcessed: number }>('index-progress', (event) => {
      indexing = true;
      progress = event.payload.currentDir.startsWith('Indexing Desktop')
        ? event.payload.currentDir
        : `${event.payload.filesProcessed.toLocaleString()} indexed`;
    });
    const cleanupUpdated = listen<IndexStats>('index-updated', (event) => {
      indexing = false;
      progress = '';
      if (query.trim()) void search(query.trim());
    });
    const cleanupReady = listen<{ totalFiles: number }>('index-ready', (event) => {
      indexing = false;
      indexedCount = event.payload.totalFiles;
      progress = '';
      if (query.trim()) void search(query.trim());
    });
    const cleanupReset = listen('spotlight-reset', resetLauncher);
    void invoke<number>('cache_size').then((count) => { indexedCount = count; }).catch(() => {});
    return () => {
    clearTimeout(debounceTimer);
    clearTimeout(metadataParseTimer);
      void cleanupProgress.then((cleanup) => cleanup());
      void cleanupUpdated.then((cleanup) => cleanup());
      void cleanupReady.then((cleanup) => cleanup());
      void cleanupReset.then((cleanup) => cleanup());
    };
  });
</script>

<div class="surface">
  <div class="top-row">
    <div class="search-row">
      <Search class="search-icon" size={22} strokeWidth={1.75} />
      {#if filterChips.length}
        <div class="chips" aria-label="Active filters">
          {#each filterChips as chip (chip.kind)}
            <button class="chip" on:click={() => removeFilter(chip.kind)} aria-label={`Remove ${chip.label} filter`}>
              <span>{chip.label}</span><X size={12} strokeWidth={2} />
            </button>
          {/each}
        </div>
      {/if}
      <input bind:this={input} bind:value={query} on:input={scheduleSearch} on:keydown={keydown} placeholder="Find files, folders, or create…" aria-label="Search Eidos" autocomplete="off" spellcheck="false" />
    </div>
    <button class:active-filter={filtersActive} class="settings-button filter-button" on:click={toggleFilters} aria-label="Open filters">
      <Filter size={20} strokeWidth={1.75} />
      {#if filtersActive}<span class="filter-dot"></span>{/if}
    </button>
    <button class:active-panel={settingsOpen} class="settings-button" on:click={toggleSettings} aria-label="Open settings"><Settings size={20} strokeWidth={1.7} /></button>
  </div>

  {#if filterOpen}
    <div class="filter-wrap">
      <FilterPanel filters={filters} on:apply={applyFilters} on:clear={clearFilters} />
    </div>
  {:else if settingsOpen}
    <div class="filter-wrap">
      <SettingsPanel on:close={closeSettings} on:saved={closeSettings} />
    </div>
  {:else if query.trim() || filtersActive}
    {#if showCreatePrompt}
      <div class="creation-card">
        <CreationPrompt
          query={query.trim()}
          filename={createFilename}
          extension={createExtension}
          {creating}
          on:filenameChange={(event) => createFilename = event.detail}
          on:extensionChange={(event) => createExtension = event.detail}
          on:cancel={() => { showCreatePrompt = false; void resizeWindow('results'); }}
          on:create={createFile}
        />
      </div>
    {:else}
      <div class="results-dropdown">
        {#if filtersActive}
          <div class="section-label">Filtered by: {filterChips.map((chip) => chip.label).join(' · ')}</div>
        {/if}

        {#if results.length > 0}
          <div class="section-label">Files &amp; folders</div>
          {#each results as result, index (result.path)}
            {@const parts = matchParts(result.filename)}
            <button class:selected={selected === index} on:mouseenter={() => selected = index} on:click={(event) => clickResult(event, result)} on:auxclick={(event) => clickResult(event, result)}>
              <span class="source">{#if result.isDirectory}<Folder size={17} strokeWidth={1.5} />{:else}<File size={17} strokeWidth={1.5} />{/if}</span>
              <span class="details"><strong>{parts.before}<b>{parts.match}</b>{parts.after}</strong><small>{result.path}</small></span>
            </button>
          {/each}
        {/if}

        {#if contentResults.length > 0}
          <div class="section-label">Content matches</div>
          {#each contentResults as result, contentIndex (result.path)}
            {@const index = results.length + contentIndex}
            <button class:selected={selected === index} on:mouseenter={() => selected = index} on:click={(event) => clickResult(event, result)} on:auxclick={(event) => clickResult(event, result)}>
              <span class="source content-source"><FileSearch size={17} strokeWidth={1.6} /></span>
              <span class="details"><strong>{result.filename}</strong><small>{result.path}</small></span>
            </button>
          {/each}
        {/if}

        {#if results.length === 0 && contentResults.length === 0 && (searching || indexing)}
          <button class="void selected" disabled>
            <span class="source"><LoaderCircle size={17} class="spinner" /></span>
            <span class="details"><strong>{indexing ? 'Indexing…' : 'Searching…'}</strong><small>{progress || 'Preparing the local index.'}</small></span>
          </button>
        {:else if results.length === 0 && contentResults.length === 0 && !filtersActive}
          <button class="void selected" on:click={prepareCreatePrompt}>
            <span class="source forge"><Sparkles size={17} /></span>
            <span class="details"><strong>No results. Create a new file?</strong><small>Press Enter or click to choose the file type.</small></span>
          </button>
        {:else if results.length === 0 && contentResults.length === 0}
          <button class="void selected" disabled>
            <span class="source"><Filter size={17} strokeWidth={1.6} /></span>
            <span class="details"><strong>No files match these filters.</strong><small>Clear a chip or adjust the filter panel.</small></span>
          </button>
        {/if}

        {#if indexedCount > 0 && !indexing}
          <div class="index-status">{indexedCount.toLocaleString()} files indexed</div>
        {/if}

      </div>
    {/if}
  {/if}
  <ToastProvider />
</div>

<style>
  .surface { width: min(572px, calc(100vw - 22px)); overflow: visible; pointer-events: auto; color: #2e3b43; background: transparent; font-family: "Segoe UI Variable Display", "SF Pro Display", "Segoe UI Variable", "Segoe UI", system-ui, sans-serif; animation: fadeIn 150ms cubic-bezier(.2,.8,.2,1); }
  .top-row { display: flex; align-items: center; gap: 8px; margin-top: 7px; }
  .search-row { width: min(444px, calc(100vw - 142px)); height: 56px; display: flex; align-items: center; gap: 10px; padding: 0 19px; border: 1px solid rgba(83,104,111,.34); border-radius: 22px; background: linear-gradient(112deg, rgba(251,254,255,.975), rgba(233,245,248,.955)); box-shadow: 0 13px 30px rgba(30,43,49,.18), inset 0 1px 0 rgba(255,255,255,.98); backdrop-filter: blur(30px) saturate(145%); -webkit-backdrop-filter: blur(30px) saturate(145%); transition: border-color 140ms ease, box-shadow 140ms ease, transform 140ms ease; }
  .search-row:focus-within { border-color: rgba(70,125,145,.5); box-shadow: 0 15px 34px rgba(30,43,49,.2), 0 0 0 3px rgba(91,151,174,.09), inset 0 1px 0 rgba(255,255,255,.98); }
  :global(.search-icon) { color: #657981; flex: 0 0 auto; }
  input { min-width: 0; flex: 1; border: 0; outline: 0; color: #2d3940; caret-color: #3088ad; background: transparent; font-size: 19px; font-weight: 430; line-height: 1; letter-spacing: -.018em; padding: 0; }
  input::placeholder { color: #8f9da2; opacity: .92; }
  .chips { min-width: 0; max-width: 240px; display: flex; align-items: center; gap: 6px; overflow: hidden; }
  .chip { max-width: 110px; height: 24px; display: inline-flex; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid rgba(80,110,120,.18); border-radius: 999px; color: #42606a; background: rgba(255,255,255,.42); font-size: 11px; font-weight: 700; white-space: nowrap; cursor: pointer; }
  .chip span { overflow: hidden; text-overflow: ellipsis; }
  .settings-button { position: relative; width: 56px; height: 56px; display: grid; place-items: center; flex: 0 0 56px; padding: 0; border: 1px solid rgba(83,104,111,.34); border-radius: 20px; color: #657981; background: linear-gradient(138deg, rgba(251,254,255,.975), rgba(233,245,248,.955)); box-shadow: 0 13px 30px rgba(30,43,49,.17), inset 0 1px 0 rgba(255,255,255,.98); backdrop-filter: blur(30px) saturate(145%); -webkit-backdrop-filter: blur(30px) saturate(145%); cursor: pointer; transition: color 120ms ease, background 120ms ease, transform 120ms ease, box-shadow 120ms ease; }
  .settings-button:hover { color: #36535e; background: rgba(252,255,255,.99); box-shadow: 0 15px 32px rgba(30,43,49,.2), inset 0 1px 0 #fff; transform: translateY(-1px); }
  .settings-button:active { transform: scale(.94); }
  .settings-button:focus-visible { outline: 2px solid rgba(0,122,255,.8); outline-offset: 2px; }
  .settings-button.active-filter, .settings-button.active-panel { color: #2f7da4; }
  .filter-dot { position: absolute; right: 10px; top: 10px; width: 8px; height: 8px; border: 2px solid rgba(247,253,255,.98); border-radius: 50%; background: #3289b1; box-shadow: 0 2px 6px rgba(50,137,177,.3); }
  :global(.spinner) { color: #007aff; animation: spin 750ms linear infinite; }
  .filter-wrap { margin-top: 0; }
  .results-dropdown, .creation-card { width: min(444px, calc(100vw - 142px)); margin-top: 4px; border: 1px solid rgba(83,104,111,.24); border-radius: 17px; background: rgba(247,251,252,.985); box-shadow: 0 18px 40px rgba(29,42,48,.16), inset 0 1px 0 rgba(255,255,255,.92); backdrop-filter: blur(30px) saturate(140%); -webkit-backdrop-filter: blur(30px) saturate(140%); }
  .results-dropdown { max-height: 300px; overflow-x: hidden; overflow-y: auto; padding: 8px; scrollbar-width: thin; contain: layout paint; }
  .creation-card { overflow: hidden; }
  .results-dropdown::-webkit-scrollbar { width: 8px; }
  .results-dropdown::-webkit-scrollbar-thumb { border-radius: 999px; background: rgba(103,116,137,.32); }
  .results-dropdown > button { position: relative; isolation: isolate; width: 100%; min-height: 50px; display: flex; align-items: center; gap: 11px; padding: 6px 8px; border: 0; border-radius: 10px; overflow: hidden; color: inherit; background: transparent; text-align: left; cursor: pointer; transition: background 110ms ease, transform 110ms ease; }
  .results-dropdown > button:disabled { cursor: default; }
  .results-dropdown > button:hover, .results-dropdown > button.selected { background: rgba(62,132,160,.1); }
  .results-dropdown > button:active:not(:disabled) { transform: scale(.992); }
  .results-dropdown > button:focus-visible { outline: 2px solid rgba(50,137,177,.5); outline-offset: -2px; }
  .source { width: 30px; height: 30px; display: grid; place-items: center; flex: 0 0 30px; border-radius: 9px; color: #596b73; background: rgba(89,113,123,.08); }
  .source.forge { color: #fff; background: linear-gradient(135deg,#348eb6,#696ed7); }
  .source.content-source { color: #2578a6; background: rgba(37,120,166,.11); }
  .section-label { height: 23px; display: flex; align-items: center; padding: 3px 8px 0; color: #7a898f; font-size: 9px; font-weight: 680; letter-spacing: .055em; text-transform: uppercase; }
  .index-status { padding: 7px 8px 3px; color: #849298; font-size: 10px; font-weight: 500; text-align: right; }
  .details { min-width: 0; flex: 1 1 auto; overflow: hidden; contain: inline-size; }
  .details strong,.details small { display: block; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; word-break: normal; }
  .details strong { font-size: 13px; font-weight: 620; line-height: 17px; }
  .details strong b { font-weight: 800; }
  .details small { margin-top: 1px; color: #788389; font-size: 10px; font-weight: 400; line-height: 14px; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes fadeIn { from { opacity: 0; transform: translateY(-4px) scale(.992); } to { opacity: 1; transform: none; } }
</style>
