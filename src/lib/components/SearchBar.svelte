<script lang="ts">
  import { onDestroy } from 'svelte';
  import { searchFlow } from '../actions';

  let query = $state('');
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    query = target.value;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      searchFlow(query);
    }, 250);
  }

  function handleClear() {
    clearTimeout(debounceTimer);
    query = '';
    searchFlow('');
  }

  onDestroy(() => clearTimeout(debounceTimer));
</script>

<div class="search-bar">
  <svg viewBox="0 0 16 16" width="13" height="13" class="search-icon" aria-hidden="true">
    <circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.5" />
    <line x1="10.5" y1="10.5" x2="14" y2="14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
  </svg>
  <input
    type="text"
    placeholder="Search..."
    value={query}
    oninput={handleInput}
    class="search-input"
  />
  {#if query}
    <button onclick={handleClear} class="clear-btn" title="Clear search">
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
        <line x1="4" y1="4" x2="12" y2="12" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
        <line x1="12" y1="4" x2="4" y2="12" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      </svg>
    </button>
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