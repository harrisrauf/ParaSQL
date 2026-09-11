<script lang="ts">
  import { onMount } from 'svelte';

  let { value, dtype, onSave, onCancel }: {
    value: string | number | boolean | null;
    dtype: string;
    onSave: (val: string | number | boolean | null) => void;
    onCancel: () => void;
  } = $props();

  let inputValue = $state('');
  let inputEl: HTMLInputElement | HTMLSelectElement | undefined = $state();
  let committed = false;

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') save();
    else if (e.key === 'Escape') {
      e.stopPropagation();
      onCancel();
    }
  }

  function save() {
    if (committed) return;
    committed = true;
    if (dtype.toLowerCase().includes('int') || dtype.toLowerCase().includes('float')) {
      const num = inputValue === '' ? null : Number(inputValue);
      onSave(isNaN(num as number) ? null : num);
    } else if (dtype.toLowerCase() === 'boolean') {
      onSave(inputValue === 'true');
    } else {
      onSave(inputValue === '' ? null : inputValue);
    }
  }

  function formatValue(v: string | number | boolean | null): string {
    if (v === null) return '';
    if (typeof v === 'boolean') return v ? 'true' : 'false';
    return String(v);
  }

  onMount(() => {
    inputValue = formatValue(value);
    if (inputEl) {
      inputEl.focus();
      if (inputEl instanceof HTMLInputElement) inputEl.select();
    }
  });
</script>

{#if dtype.toLowerCase() === 'boolean'}
  <select
    bind:this={inputEl as HTMLSelectElement}
    bind:value={inputValue}
    onchange={save}
    onkeydown={handleKeydown}
    class="cell-editor"
  >
    <option value="true">true</option>
    <option value="false">false</option>
  </select>
{:else}
  <input
    bind:this={inputEl as HTMLInputElement}
    type={dtype.toLowerCase().includes('int') || dtype.toLowerCase().includes('float') ? 'number' : 'text'}
    bind:value={inputValue}
    onkeydown={handleKeydown}
    onblur={save}
    class="cell-editor"
  />
{/if}

<style>
  .cell-editor {
    width: 100%;
    height: 100%;
    border: 2px solid #1a73e8;
    outline: none;
    padding: 2px 6px;
    font-size: 13px;
    font-family: inherit;
    background: white;
    box-sizing: border-box;
    border-radius: 2px;
  }
  select.cell-editor {
    padding: 1px 4px;
  }
</style>
