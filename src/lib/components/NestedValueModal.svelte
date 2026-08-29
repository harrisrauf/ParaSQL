<script lang="ts">
  let { value, columnName, onClose }: {
    value: string;
    columnName: string;
    onClose: () => void;
  } = $props();

  let formattedJson = $derived(
    (() => {
      try {
        return JSON.stringify(JSON.parse(value), null, 2);
      } catch {
        return value;
      }
    })()
  );
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onClose()} />

<div class="modal-overlay" onclick={onClose} role="dialog">
  <div class="modal-content" onclick={(e) => e.stopPropagation()}>
    <div class="modal-header">
      <h3>{columnName}</h3>
      <button onclick={onClose} class="close-btn">✕</button>
    </div>
    <pre class="modal-body"><code>{formattedJson}</code></pre>
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal-content {
    background: white;
    border-radius: 8px;
    min-width: 400px;
    max-width: 80vw;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0,0,0,0.2);
  }
  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-bottom: 1px solid #eee;
  }
  .modal-header h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .close-btn {
    border: none;
    background: none;
    cursor: pointer;
    font-size: 16px;
    color: #999;
    padding: 4px;
  }
  .close-btn:hover {
    color: #333;
  }
  .modal-body {
    padding: 16px;
    overflow: auto;
    margin: 0;
    font-family: 'SF Mono', 'Fira Code', monospace;
    font-size: 13px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
