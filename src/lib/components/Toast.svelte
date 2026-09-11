<script lang="ts">
  import { toastStore } from '../stores/ui';
</script>

{#if $toastStore.length > 0}
  <div class="toast-stack" role="status" aria-live="polite">
    {#each $toastStore as t (t.id)}
      <button class="toast toast-{t.kind}" onclick={() => toastStore.dismiss(t.id)}>
        {t.message}
      </button>
    {/each}
  </div>
{/if}

<style>
  .toast-stack {
    position: fixed;
    right: 16px;
    bottom: 44px;
    z-index: 2000;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 380px;
  }

  .toast {
    display: block;
    width: 100%;
    text-align: left;
    padding: 9px 12px;
    border-radius: 6px;
    border: 1px solid var(--border-color, #e0e0e0);
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.16);
  }

  .toast-error {
    border-color: #e5a5a5;
    background: #fdf2f2;
    color: #a12626;
  }

  .toast-success {
    border-color: #a5d6a5;
    background: #f2fdf2;
    color: #26682b;
  }
</style>
