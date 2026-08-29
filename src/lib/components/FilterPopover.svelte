<script lang="ts">
  import { onMount } from 'svelte';
  import { tableStore, distinctValues } from '../stores/table';

  let { colName, x, y, onClose }: {
    colName: string;
    x: number;
    y: number;
    onClose: () => void;
  } = $props();

  let colIdx = $derived($tableStore.columns.findIndex(c => c.name === colName));
  let values = $derived(colIdx >= 0 ? distinctValues($tableStore.rows, colIdx) : []);
  let selected = $derived($tableStore.filters[colName] ?? new Set<string | null>());
  let search = $state('');
  let visibleValues = $derived(
    search
      ? values.filter(v => v !== null && v.toLowerCase().includes(search.toLowerCase()))
      : values
  );

  function toggle(v: string | null) {
    const next = new Set(selected);
    if (next.has(v)) next.delete(v);
    else next.add(v);
    tableStore.setFilter(colName, next);
  }

  function selectAll() {
    tableStore.setFilter(colName, new Set(values));
  }

  function selectNone() {
    tableStore.setFilter(colName, new Set());
  }

  onMount(() => {
    const close = () => onClose();
    document.addEventListener('click', close);
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('click', close);
      document.removeEventListener('keydown', key);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="filter-popover"
  role="dialog"
  aria-label="Filter {colName}"
  tabindex="-1"
  style="left: {Math.min(x, window.innerWidth - 240)}px; top: {y}px;"
  onclick={(e) => e.stopPropagation()}
>
  <div class="filter-search">
    <input type="text" bind:value={search} placeholder="Search values…" />
  </div>
  <div class="filter-actions">
    <button onclick={selectAll}>Select all</button>
    <button onclick={selectNone}>None</button>
    <span class="filter-count">{selected.size} selected</span>
  </div>
  <div class="filter-list">
    <label class="filter-option">
      <input type="checkbox" checked={selected.has(null)} onchange={() => toggle(null)} />
      <span class="null-opt">(null)</span>
    </label>
    {#each visibleValues.slice(0, 200) as v (v)}
      <label class="filter-option">
        <input type="checkbox" checked={selected.has(v)} onchange={() => toggle(v)} />
        <span class="value-opt">{v}</span>
      </label>
    {/each}
  </div>
</div>

<style>
  .filter-popover {
    position: fixed;
    z-index: 1000;
    width: 220px;
    background: var(--bg, #fff);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.18);
    padding: 6px;
    font-size: 12px;
  }

  .filter-search input {
    width: 100%;
    box-sizing: border-box;
    padding: 4px 8px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 4px;
    font-size: 12px;
    font-family: inherit;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
  }

  .filter-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0;
  }

  .filter-actions button {
    border: none;
    background: none;
    color: var(--accent-color, #1a73e8);
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
    padding: 2px 4px;
  }

  .filter-actions button:hover {
    text-decoration: underline;
  }

  .filter-count {
    margin-left: auto;
    color: var(--text-secondary, #999);
    font-size: 11px;
  }

  .filter-list {
    max-height: 280px;
    overflow: auto;
  }

  .filter-option {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 4px;
    cursor: pointer;
    border-radius: 3px;
    white-space: nowrap;
  }

  .filter-option:hover {
    background: var(--hover-bg, #f5f5f5);
  }

  .filter-option input {
    flex-shrink: 0;
  }

  .value-opt {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .null-opt {
    color: var(--text-secondary, #999);
    font-style: italic;
  }
</style>