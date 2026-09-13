<script lang="ts">
  import { onMount } from 'svelte';
  import { canUndo, canRedo } from '../commands';
  import { tableStore, selectedRowIds } from '../stores/table';
  import { settings } from '../stores/settings';
  import { uiStore } from '../stores/ui';
  import {
    openFileFlow,
    openFolderFlow,
    saveFlow,
    saveAsFlow,
    exportFlow,
    undoFlow,
    redoFlow,
    deleteSelectedFlow,
    insertRowFlow,
    copySelectionToClipboard,
    newWorkspaceFlow,
    openWorkspaceFlow,
    saveWorkspaceFlow,
    saveWorkspaceAsFlow,
    addTableFlow,
    addFolderFlow,
    closeEditorFlow,
  } from '../actions';
  import { workspaceStore } from '../stores/workspace';

  interface SubItem {
    label: string;
    action: () => void;
    checked?: boolean;
  }

  interface MenuItem {
    label?: string;
    action?: () => void;
    disabled?: boolean;
    separator?: boolean;
    checked?: boolean;
    submenu?: SubItem[];
  }

  interface MenuDef {
    id: string;
    label: string;
    items: MenuItem[];
  }

  let openMenu = $state<string | null>(null);
  let menubarEl = $state<HTMLElement | null>(null);
  let undoAvail = $state(false);
  let redoAvail = $state(false);

  let modified = $derived($tableStore.modified);
  let selectedCount = $derived($selectedRowIds.size);
  let hasData = $derived($tableStore.columns.length > 0);
  let darkMode = $derived($settings.darkMode);
  let density = $derived($settings.rowDensity);
  let sidebarVisible = $derived($uiStore.sidebarVisible);
  let recentFiles = $derived($settings.recentFiles);
  let wsDoc = $derived($workspaceStore.doc);
  let wsEditorOpen = $derived($workspaceStore.editorOpen);
  let editable = $derived($tableStore.editable);

  async function refreshEditState() {
    const [u, r] = await Promise.all([canUndo(), canRedo()]);
    undoAvail = u;
    redoAvail = r;
  }

  function toggleMenu(id: string) {
    if (id === 'edit') refreshEditState();
    openMenu = openMenu === id ? null : id;
  }

  function selectAll() {
    tableStore.selectAll();
  }

  let menus = $derived<MenuDef[]>([
    {
      id: 'file',
      label: 'File',
      items: [
        { label: 'Open File…', action: () => openFileFlow() },
        { label: 'Open Folder…', action: () => openFolderFlow() },
        { separator: true },
        { label: 'Save', action: () => saveFlow(), disabled: !modified || !hasData },
        { label: 'Save As…', action: () => saveAsFlow(), disabled: !hasData },
        { separator: true },
        {
          label: 'Export',
          disabled: !hasData,
          submenu: [
            { label: 'JSON', action: () => exportFlow('json') },
            { label: 'CSV', action: () => exportFlow('csv') },
            { label: 'Excel (XLSX)', action: () => exportFlow('excel') },
          ],
        },
        { separator: true },
        {
          label: 'Recent Files',
          disabled: recentFiles.length === 0,
          submenu: [
            ...recentFiles.slice(0, 10).map(path => ({
              label: path.split(/[\\/]/).pop() ?? path,
              action: () => openFileFlow(path),
            })),
          ],
        },
        { separator: true },
        { label: 'Quit', action: () => window.close() },
      ],
    },
    {
      id: 'workspace',
      label: 'Workspace',
      items: [
        { label: 'New Workspace…', action: () => newWorkspaceFlow() },
        { label: 'Open Workspace…', action: () => openWorkspaceFlow() },
        { separator: true },
        { label: 'Save Workspace', action: () => saveWorkspaceFlow(), disabled: !wsDoc },
        { label: 'Save Workspace As…', action: () => saveWorkspaceAsFlow(), disabled: !wsDoc },
        { separator: true },
        { label: 'Add Table…', action: () => addTableFlow(), disabled: !wsDoc },
        { label: 'Add Folder…', action: () => addFolderFlow(), disabled: !wsDoc },
        { separator: true },
        { label: 'Close Editor', action: () => closeEditorFlow(), disabled: !wsEditorOpen },
      ],
    },
    {
      id: 'edit',
      label: 'Edit',
      items: [
        { label: 'Undo', action: () => undoFlow(), disabled: !undoAvail || !editable },
        { label: 'Redo', action: () => redoFlow(), disabled: !redoAvail || !editable },
        { separator: true },
        { label: 'Copy', action: () => copySelectionToClipboard(), disabled: selectedCount === 0 },
        { label: 'Select All', action: selectAll, disabled: !hasData },
        { label: 'Delete Selected Rows', action: () => deleteSelectedFlow(), disabled: selectedCount === 0 || !editable },
        { separator: true },
        { label: 'Insert Row', action: () => insertRowFlow(), disabled: !hasData || !editable },
      ],
    },
    {
      id: 'view',
      label: 'View',
      items: [
        { label: 'Toggle Sidebar', action: () => uiStore.toggleSidebar() },
        { label: 'Dark Mode', action: () => settings.toggleDarkMode(), checked: darkMode },
        { separator: true },
        {
          label: 'Row Density',
          submenu: [
            {
              label: 'Compact',
              checked: density === 'compact',
              action: () => settings.setRowDensity('compact'),
            },
            {
              label: 'Comfortable',
              checked: density === 'comfortable',
              action: () => settings.setRowDensity('comfortable'),
            },
          ],
        },
      ],
    },
    {
      id: 'data',
      label: 'Data',
      items: [
        { label: 'Insert Row', action: () => insertRowFlow(), disabled: !hasData || !editable },
        { separator: true },
        {
          label: 'Sort',
          disabled: !hasData,
          submenu: [
            { label: 'Sort Ascending', action: () => sortCurrent('asc') },
            { label: 'Sort Descending', action: () => sortCurrent('desc') },
            { label: 'Clear Sort', action: () => tableStore.clearSort(), checked: false },
          ],
        },
        { label: 'Clear Filters', action: () => tableStore.clearFilters(), disabled: !hasData },
      ],
    },
    {
      id: 'help',
      label: 'Help',
      items: [
        { label: 'About Parquet Viewer', action: () => alert('Parquet Viewer v0.2.0\nA desktop parquet file viewer powered by DuckDB.') },
      ],
    },
  ]);

  function sortCurrent(direction: 'asc' | 'desc') {
    const current = $tableStore.sort;
    const column = current.column || $tableStore.columns[0]?.name;
    if (!column) return;
    tableStore.update(s => ({ ...s, sort: { column, direction } }));
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') openMenu = null;
  }

  onMount(() => {
    const close = () => {
      openMenu = null;
    };
    document.addEventListener('click', close);
    document.addEventListener('mousemove', handleMouseMove);
    return () => {
      document.removeEventListener('click', close);
      document.removeEventListener('mousemove', handleMouseMove);
    };
  });

  // Close the open menu once the pointer leaves both the bar and the open
  // dropdown (standard menu-bar hover-out behavior).
  function handleMouseMove(e: MouseEvent) {
    if (!openMenu) return;
    const bar = menubarEl?.getBoundingClientRect();
    if (!bar) return;
    const inside = (r: DOMRect) =>
      e.clientX >= r.left && e.clientX <= r.right &&
      e.clientY >= r.top && e.clientY <= r.bottom;
    if (inside(bar)) return;
    const panels = document.querySelectorAll('.menu-dropdown, .submenu');
    for (const panel of panels) {
      if (inside(panel.getBoundingClientRect())) return;
    }
    openMenu = null;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="menubar" role="menubar" aria-label="Main menu" tabindex="-1" bind:this={menubarEl} onclick={(e) => e.stopPropagation()}>
  {#each menus as menu (menu.id)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="menu-wrap"
      onmouseenter={() => {
        if (openMenu && openMenu !== menu.id) toggleMenu(menu.id);
      }}
    >
      <button
        class="menu-title"
        class:open={openMenu === menu.id}
        onclick={(e) => {
          e.stopPropagation();
          toggleMenu(menu.id);
        }}
      >{menu.label}</button>
      {#if openMenu === menu.id}
        <div class="menu-dropdown">
          {#each menu.items as item, itemIndex (item.label ?? `sep-${menu.id}-${itemIndex}`)}
            {#if item.separator}
              <div class="menu-sep"></div>
            {:else if item.submenu}
              <div class="menu-item has-sub" class:disabled={item.disabled}>
                <span>{item.label}</span>
                <span class="sub-arrow">▸</span>
                <div class="submenu">
                  {#each item.submenu as sub (sub.label)}
                    <button class="menu-item" onclick={(e) => { e.stopPropagation(); sub.action(); openMenu = null; }}>
                      {#if sub.checked}<span class="check">✓</span>{/if}
                      <span>{sub.label}</span>
                    </button>
                  {/each}
                </div>
              </div>
            {:else}
              <button
                class="menu-item"
                class:disabled={item.disabled}
                onclick={(e) => {
                  e.stopPropagation();
                  if (item.disabled || !item.action) return;
                  item.action();
                  openMenu = null;
                }}
              >
                {#if item.checked}<span class="check">✓</span>{/if}
                <span>{item.label}</span>
              </button>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .menubar {
    display: flex;
    align-items: stretch;
    background: var(--panel-bg, #fafafa);
    border-bottom: 1px solid var(--border-color, #e0e0e0);
    user-select: none;
    flex-shrink: 0;
    height: 28px;
    padding: 0 4px;
    position: relative;
    z-index: 100;
  }

  .menu-wrap {
    position: relative;
    display: flex;
  }

  .menu-title {
    border: none;
    background: none;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary, #333);
    padding: 4px 10px;
    cursor: pointer;
    border-radius: 4px;
  }

  .menu-title:hover,
  .menu-title.open {
    background: var(--hover-bg, #e8e8e8);
  }

  .menu-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    min-width: 200px;
    background: var(--bg, #fff);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
    padding: 4px 0;
    z-index: 1000;
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    background: none;
    font-family: inherit;
    font-size: 13px;
    color: var(--text-primary, #333);
    padding: 6px 14px;
    cursor: pointer;
    text-align: left;
    white-space: nowrap;
    position: relative;
  }

  .menu-item:hover {
    background: var(--hover-bg, #f0f0f0);
  }

  .menu-item.disabled {
    color: var(--text-secondary, #aaa);
    cursor: default;
  }

  .menu-item.disabled:hover {
    background: none;
  }

  .menu-item.has-sub {
    position: relative;
    justify-content: space-between;
  }

  .sub-arrow {
    font-size: 10px;
    color: var(--text-secondary, #888);
  }

  .submenu {
    display: none;
    position: absolute;
    left: 100%;
    top: -4px;
    min-width: 160px;
    background: var(--bg, #fff);
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
    padding: 4px 0;
  }

  .menu-item.has-sub:hover .submenu {
    display: block;
  }

  /* Invisible bridge so the pointer can travel from the parent item into the
     submenu without leaving the hover target. */
  .submenu::before {
    content: '';
    position: absolute;
    left: -10px;
    top: 0;
    bottom: 0;
    width: 10px;
  }

  .menu-sep {
    height: 1px;
    margin: 4px 8px;
    background: var(--border-color, #e0e0e0);
  }

  .check {
    position: absolute;
    left: 4px;
    top: 50%;
    transform: translateY(-50%);
    width: 12px;
    font-size: 11px;
    color: var(--text-primary, #333);
  }
</style>