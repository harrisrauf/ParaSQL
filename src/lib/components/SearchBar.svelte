<script lang="ts">
  import { onDestroy } from 'svelte';
  import { tableStore } from '../stores/table';

  let query = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    query = target.value;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      tableStore.setSearch(query, null);
    }, 200);
  }

  function handleClear() {
    clearTimeout(debounceTimer);
    query = '';
    tableStore.setSearch('', null);
  }

  onDestroy(() => clearTimeout(debounceTimer));
</script>

<div class="search-bar">
  <span class="search-icon">🔍</span>
  <input
    type="text"
    placeholder="Search..."
    value={query}
    oninput={handleInput}
    class="search-input"
  />
  {#if query}
    <button onclick={handleClear} class="clear-btn">✕</button>
  {/if}
</div>

<style>
  .search-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--bg, white);
    border: 1px solid var(--border-color, #ddd);
    border-radius: 4px;
    padding: 2px 8px;
    min-width: 200px;
  }
  .search-icon {
    font-size: 12px;
    opacity: 0.5;
  }
  .search-input {
    border: none;
    outline: none;
    padding: 4px 0;
    font-size: 13px;
    flex: 1;
    background: transparent;
    color: var(--text-primary, #333);
    font-family: inherit;
  }
  .search-input::placeholder {
    color: var(--text-secondary, #999);
  }
  .clear-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 12px;
    padding: 2px;
    color: var(--text-secondary, #999);
  }
  .clear-btn:hover {
    color: var(--text-primary, #333);
  }
</style>