<script lang="ts">
  let { onClose }: { onClose: () => void } = $props();

  interface Step {
    icon: string;
    title: string;
    body: string;
    note?: string;
  }

  const steps: Step[] = [
    {
      icon: 'folder',
      title: 'Open your files',
      body: 'Use File → Open File… for a single Parquet file, or Open Folder to load every Parquet file in a folder as one table.',
      note: 'Single files open in the full editor — every cell is editable.',
    },
    {
      icon: 'grid',
      title: 'Explore the grid',
      body: 'Click a table in the sidebar to preview it. Sort by clicking a header, filter with the funnel icon, or search the whole table. The grid is virtualized, so half a million rows scroll smoothly.',
    },
    {
      icon: 'pencil',
      title: 'Edit data',
      body: 'Double-click a cell (or press F2) to edit it. Add rows with + Row, delete with − Rows, and manage columns from the Schema tab. Every change is undoable with Ctrl+Z and redoable with Ctrl+Y.',
    },
    {
      icon: 'save',
      title: 'Save & export',
      body: 'Ctrl+S writes your changes back to Parquet atomically — a failed save never corrupts the file. Export the table, or any query result, to Parquet, CSV, JSON, or Excel from the toolbar and result bar.',
    },
    {
      icon: 'layers',
      title: 'Build a workspace',
      body: 'Workspace → New Workspace, then Add Table… or Add Folder… — every file becomes a SQL table. Save it as a portable .parasql file with relative paths you can commit or share.',
      note: 'Workspace tables are query-only for now; multi-file editing is on the roadmap.',
    },
    {
      icon: 'terminal',
      title: 'Query across files',
      body: 'Open the Query tab and join tables with full DuckDB SQL — CTEs, window functions, UNNEST, quoted identifiers. Ctrl+Enter runs the query; recent queries stay in history; copy or export results right from the result bar.',
    },
    {
      icon: 'sparkle',
      title: 'Chart your results',
      body: 'After running a query, open the Charts tab: pick a chart type (bar, line, area, pie, scatter), choose the X and Y columns, optionally split by a series — then save the chart into your workspace.',
      note: 'Saved charts re-run their query and redraw whenever you reopen the workspace.',
    },
    {
      icon: 'sparkle',
      title: 'Coming soon',
      body: 'Pivot tables, dashboards and notebooks to compose your charts, and AI-assisted dataset summaries — all still local and private.',
    },
  ];

  let index = $state(0);

  function next() {
    if (index < steps.length - 1) index += 1;
  }
  function prev() {
    if (index > 0) index -= 1;
  }
  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      onClose();
      return;
    }
    if (e.key === 'ArrowRight') next();
    if (e.key === 'ArrowLeft') prev();
  }
</script>

{#snippet icon(name: string)}
  {#if name === 'folder'}
    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </svg>
  {:else if name === 'grid'}
    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="3" width="18" height="18" rx="2" />
      <path d="M3 9h18M3 15h18M9 3v18M15 3v18" />
    </svg>
  {:else if name === 'pencil'}
    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M4 20h4L19 9a2.1 2.1 0 0 0-3-3L5 17z" />
      <path d="M13.5 6.5l3 3" />
    </svg>
  {:else if name === 'save'}
    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 3v12" />
      <path d="M7 10l5 5 5-5" />
      <path d="M5 21h14" />
    </svg>
  {:else if name === 'layers'}
    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <ellipse cx="12" cy="5" rx="8" ry="3" />
      <path d="M4 5v14c0 1.7 3.6 3 8 3s8-1.3 8-3V5" />
      <path d="M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" />
    </svg>
  {:else if name === 'terminal'}
    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M7 9l3 3-3 3" />
      <path d="M13 15h4" />
    </svg>
  {:else}
    <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M12 3l1.8 4.9L19 9.7l-5.2 1.8L12 16.4l-1.8-4.9L5 9.7l5.2-1.8z" />
      <path d="M19 15l.9 2.4 2.1.9-2.1.9L19 21l-.9-1.8-2.1-.9 2.1-.9z" />
    </svg>
  {/if}
{/snippet}

<svelte:window onkeydown={handleKey} />

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div class="help-backdrop" onclick={onClose}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="help-modal"
    role="dialog"
    aria-modal="true"
    aria-label="How to use ParaSQL"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="help-header">
      <span class="help-title">How to use ParaSQL</span>
      <button class="help-close" aria-label="Close" onclick={onClose}>×</button>
    </div>

    <div class="help-body">
      <div class="step-icon" aria-hidden="true">
        {@render icon(steps[index].icon)}
      </div>
      <h3>{steps[index].title}</h3>
      <p>{steps[index].body}</p>
      {#if steps[index].note}
        <p class="step-note">{steps[index].note}</p>
      {/if}
    </div>

    <div class="help-footer">
      <button class="nav-btn" onclick={prev} disabled={index === 0}>← Back</button>
      <div class="dots">
        {#each steps as s, i (s.title)}
          <button
            class="dot"
            class:active={i === index}
            aria-label="Step {i + 1}"
            onclick={() => (index = i)}
          ></button>
        {/each}
      </div>
      {#if index < steps.length - 1}
        <button class="nav-btn primary" onclick={next}>Next →</button>
      {:else}
        <button class="nav-btn primary" onclick={onClose}>Get started</button>
      {/if}
    </div>

    <div class="step-count">Step {index + 1} of {steps.length}</div>
  </div>
</div>

<style>
  .help-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 1600;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .help-modal {
    width: min(520px, 92vw);
    background: var(--bg, #fff);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 10px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.35);
    padding: 16px 18px 12px;
    color: var(--text-primary, #333);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .help-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .help-title {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text-secondary, #888);
  }

  .help-close {
    border: none;
    background: none;
    color: var(--text-secondary, #888);
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
  }

  .help-close:hover {
    background: var(--hover-bg, #f0f0f0);
    color: var(--text-primary, #333);
  }

  .help-body {
    min-height: 190px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  .step-icon {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent-color, #1a73e8);
    background: color-mix(in srgb, var(--accent-color, #1a73e8) 12%, transparent);
  }

  .help-body h3 {
    margin: 2px 0 0;
    font-size: 17px;
  }

  .help-body p {
    margin: 0;
    font-size: 13px;
    line-height: 1.55;
    color: var(--text-primary, #333);
  }

  .step-note {
    font-size: 12px !important;
    color: var(--text-secondary, #888) !important;
  }

  .help-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-top: 1px solid var(--border-subtle, #f0f0f0);
    padding-top: 10px;
  }

  .nav-btn {
    padding: 5px 12px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 5px;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .nav-btn:hover:not(:disabled) {
    background: var(--hover-bg, #f0f0f0);
  }

  .nav-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .nav-btn.primary {
    background: var(--accent-color, #1a73e8);
    border-color: var(--accent-color, #1a73e8);
    color: var(--accent-text, #fff);
    font-weight: 500;
  }

  .nav-btn.primary:hover:not(:disabled) {
    background: var(--accent-hover, #1557b0);
    border-color: var(--accent-hover, #1557b0);
  }

  .dots {
    display: flex;
    gap: 6px;
  }

  .dot {
    width: 8px;
    height: 8px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: var(--border-color, #ddd);
    cursor: pointer;
  }

  .dot.active {
    background: var(--accent-color, #1a73e8);
  }

  .step-count {
    text-align: center;
    font-size: 11px;
    color: var(--text-secondary, #999);
  }
</style>
