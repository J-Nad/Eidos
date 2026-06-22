<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { FileKind, MetadataFilters } from '$lib/types';
  import { emptyMetadataFilters, normalizeFilters } from '$lib/stores/metadataFilters';

  export let filters: MetadataFilters = { ...emptyMetadataFilters, extensions: [] };

  const dispatch = createEventDispatcher<{
    apply: MetadataFilters;
    clear: void;
  }>();

  const extensions = ['pdf', 'docx', 'xlsx', 'csv', 'pptx', 'jpg', 'png', 'gif', 'mp4', 'mov', 'mp3', 'zip', 'py', 'js', 'ts', 'rs', 'go', 'json', 'md', 'txt'];
  const kinds: { label: string; value: FileKind }[] = [
    { label: 'Document', value: 'document' },
    { label: 'Spreadsheet', value: 'spreadsheet' },
    { label: 'Image', value: 'image' },
    { label: 'Video', value: 'video' },
    { label: 'Audio', value: 'audio' },
    { label: 'Code', value: 'code' },
    { label: 'Archive', value: 'archive' },
    { label: 'Folder', value: 'folder' }
  ];

  let extension = '';
  let kind = '';
  let minSize = '';
  let maxSize = '';
  let unit: 'KB' | 'MB' | 'GB' = 'MB';
  let modifiedPreset = '';
  let createdPreset = '';
  let modifiedStart = '';
  let modifiedEnd = '';
  let createdStart = '';
  let createdEnd = '';

  $: {
    const current = normalizeFilters(filters);
    extension = current.extensions?.[0] ?? '';
    kind = current.kind ?? '';
    minSize = current.sizeMin ? String(bytesToUnit(current.sizeMin, unit)) : '';
    maxSize = current.sizeMax ? String(bytesToUnit(current.sizeMax, unit)) : '';
    modifiedStart = current.modifiedAfter ? dateInput(current.modifiedAfter) : '';
    modifiedEnd = current.modifiedBefore ? dateInput(current.modifiedBefore) : '';
    createdStart = current.createdAfter ? dateInput(current.createdAfter) : '';
    createdEnd = current.createdBefore ? dateInput(current.createdBefore) : '';
  }

  function bytesToUnit(bytes: number, selectedUnit: 'KB' | 'MB' | 'GB') {
    const divisor = selectedUnit === 'GB' ? 1024 ** 3 : selectedUnit === 'MB' ? 1024 ** 2 : 1024;
    return Math.round((bytes / divisor) * 100) / 100;
  }

  function unitToBytes(value: string) {
    const amount = Number(value);
    if (!Number.isFinite(amount) || amount <= 0) return null;
    const multiplier = unit === 'GB' ? 1024 ** 3 : unit === 'MB' ? 1024 ** 2 : 1024;
    return Math.round(amount * multiplier);
  }

  function startOfDay(date: Date) {
    const copy = new Date(date);
    copy.setHours(0, 0, 0, 0);
    return Math.floor(copy.getTime() / 1000);
  }

  function endOfDay(date: Date) {
    const copy = new Date(date);
    copy.setHours(23, 59, 59, 999);
    return Math.floor(copy.getTime() / 1000);
  }

  function dateInput(timestamp: number) {
    const date = new Date(timestamp * 1000);
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
  }

  function dateRangeForPreset(preset: string): [number | null, number | null] {
    const now = new Date();
    if (!preset) return [null, null];
    if (preset === 'today') return [startOfDay(now), endOfDay(now)];
    if (preset === 'yesterday') {
      const day = new Date(now);
      day.setDate(day.getDate() - 1);
      return [startOfDay(day), endOfDay(day)];
    }
    if (preset === 'week') {
      const start = new Date(now);
      const day = start.getDay() || 7;
      start.setDate(start.getDate() - day + 1);
      return [startOfDay(start), endOfDay(now)];
    }
    if (preset === 'month') {
      const start = new Date(now.getFullYear(), now.getMonth(), 1);
      return [startOfDay(start), endOfDay(now)];
    }
    if (preset === 'past7') {
      const start = new Date(now);
      start.setDate(start.getDate() - 7);
      return [startOfDay(start), endOfDay(now)];
    }
    return [null, null];
  }

  function customDateRange(start: string, end: string): [number | null, number | null] {
    if (!start && !end) return [null, null];
    return [
      start ? startOfDay(new Date(`${start}T00:00:00`)) : null,
      end ? endOfDay(new Date(`${end}T00:00:00`)) : null
    ];
  }

  function apply() {
    const [modifiedAfterPreset, modifiedBeforePreset] = dateRangeForPreset(modifiedPreset);
    const [createdAfterPreset, createdBeforePreset] = dateRangeForPreset(createdPreset);
    const [modifiedAfterCustom, modifiedBeforeCustom] = customDateRange(modifiedStart, modifiedEnd);
    const [createdAfterCustom, createdBeforeCustom] = customDateRange(createdStart, createdEnd);

    dispatch('apply', normalizeFilters({
      extensions: extension ? [extension] : [],
      kind: kind ? (kind as FileKind) : null,
      sizeMin: unitToBytes(minSize),
      sizeMax: unitToBytes(maxSize),
      modifiedAfter: modifiedAfterPreset ?? modifiedAfterCustom,
      modifiedBefore: modifiedBeforePreset ?? modifiedBeforeCustom,
      createdAfter: createdAfterPreset ?? createdAfterCustom,
      createdBefore: createdBeforePreset ?? createdBeforeCustom
    }));
  }

  function clear() {
    extension = '';
    kind = '';
    minSize = '';
    maxSize = '';
    modifiedPreset = '';
    createdPreset = '';
    modifiedStart = '';
    modifiedEnd = '';
    createdStart = '';
    createdEnd = '';
    dispatch('clear');
  }
</script>

<div class="filter-card">
  <div class="grid">
    <label>
      <span>File type</span>
      <select bind:value={extension}>
        <option value="">Any extension</option>
        {#each extensions as item}
          <option value={item}>.{item}</option>
        {/each}
      </select>
    </label>
    <label>
      <span>Kind</span>
      <select bind:value={kind}>
        <option value="">Any kind</option>
        {#each kinds as item}
          <option value={item.value}>{item.label}</option>
        {/each}
      </select>
    </label>

    <label>
      <span>Min size</span>
      <input bind:value={minSize} inputmode="decimal" placeholder="0" />
    </label>
    <label>
      <span>Max size</span>
      <input bind:value={maxSize} inputmode="decimal" placeholder="Any" />
    </label>
    <label>
      <span>Unit</span>
      <select bind:value={unit}>
        <option>KB</option>
        <option>MB</option>
        <option>GB</option>
      </select>
    </label>

    <label class="wide date-field">
      <span>Modified date</span>
      <small>Last edited, renamed, or changed.</small>
      <select bind:value={modifiedPreset}>
        <option value="">Custom / Any</option>
        <option value="today">Today</option>
        <option value="yesterday">Yesterday</option>
        <option value="week">This Week</option>
        <option value="month">This Month</option>
        <option value="past7">Past 7 Days</option>
      </select>
    </label>
    <label>
      <span>From</span>
      <input bind:value={modifiedStart} type="date" />
    </label>
    <label>
      <span>To</span>
      <input bind:value={modifiedEnd} type="date" />
    </label>

    <label class="wide date-field">
      <span>Created date</span>
      <small>When the file was first created on disk.</small>
      <select bind:value={createdPreset}>
        <option value="">Custom / Any</option>
        <option value="today">Today</option>
        <option value="yesterday">Yesterday</option>
        <option value="week">This Week</option>
        <option value="month">This Month</option>
        <option value="past7">Past 7 Days</option>
      </select>
    </label>
    <label>
      <span>From</span>
      <input bind:value={createdStart} type="date" />
    </label>
    <label>
      <span>To</span>
      <input bind:value={createdEnd} type="date" />
    </label>
  </div>

  <div class="actions">
    <button class="clear" on:click={clear}>Clear all</button>
    <button class="apply" on:click={apply}>Apply</button>
  </div>
</div>

<style>
  .filter-card { width: min(572px, calc(100vw - 22px)); padding: 14px; border: 1px solid rgba(83,104,111,.24); border-radius: 18px; background: rgba(247,251,252,.985); box-shadow: 0 18px 40px rgba(29,42,48,.16); backdrop-filter: blur(30px) saturate(140%); -webkit-backdrop-filter: blur(30px) saturate(140%); animation: drop 140ms ease-out; }
  .grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  label { min-width: 0; display: grid; gap: 5px; color: #34444b; font-size: 11px; font-weight: 650; }
  label.wide { grid-column: span 1; }
  span { color: #64747b; letter-spacing: .01em; }
  .date-field small { margin-top: -2px; overflow: hidden; color: #849299; font-size: 9px; font-weight: 600; line-height: 1.2; text-overflow: ellipsis; white-space: nowrap; }
  input, select { width: 100%; min-width: 0; height: 34px; border: 1px solid rgba(80,110,120,.22); border-radius: 12px; padding: 0 10px; color: #34444b; background: rgba(255,255,255,.58); outline: none; font: inherit; font-weight: 500; }
  input:focus, select:focus { border-color: rgba(0,122,255,.45); box-shadow: 0 0 0 3px rgba(0,122,255,.08); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  button { height: 34px; border: 0; border-radius: 999px; padding: 0 14px; font-size: 12px; font-weight: 700; cursor: pointer; transition: transform 120ms ease, background 120ms ease; }
  button:active { transform: scale(.96); }
  .clear { color: #53656c; background: rgba(102,116,140,.12); }
  .apply { color: white; background: linear-gradient(135deg,#007aff,#5b7cff); box-shadow: 0 8px 18px rgba(0,122,255,.2); }
  @keyframes drop { from { opacity: 0; transform: translateY(-4px) scale(.992); } to { opacity: 1; transform: none; } }
</style>
