<script lang="ts">
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
    query = '';
    tableStore.setSearch('', null);
  }
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
    background: white;
    border: 1px solid #ddd;
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
    font-family: inherit;
  }
  .clear-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 12px;
    padding: 2px;
    color: #999;
  }
  .clear-btn:hover {
    color: #333;
  }
</style>
