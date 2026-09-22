<script lang="ts">
  import { onMount } from 'svelte';
  import * as echarts from 'echarts/core';
  import { BarChart, LineChart, PieChart, ScatterChart } from 'echarts/charts';
  import { GridComponent, LegendComponent, TooltipComponent } from 'echarts/components';
  import { CanvasRenderer } from 'echarts/renderers';
  import type { EChartsCoreOption, EChartsType } from 'echarts/core';
  import { tableStore } from '../stores/table';
  import { workspaceStore } from '../stores/workspace';
  import { settings } from '../stores/settings';
  import { notify } from '../stores/ui';
  import { deleteChartFlow, saveChartFlow } from '../actions';
  import { executeSql } from '../commands';
  import type { ChartConfig, ChartType, ColumnInfo } from '../types';

  echarts.use([
    BarChart, LineChart, PieChart, ScatterChart,
    GridComponent, LegendComponent, TooltipComponent, CanvasRenderer,
  ]);

  const CHART_TYPES: { value: ChartType; label: string }[] = [
    { value: 'bar', label: 'Bar' },
    { value: 'line', label: 'Line' },
    { value: 'area', label: 'Area' },
    { value: 'pie', label: 'Pie' },
    { value: 'scatter', label: 'Scatter' },
  ];

  let columns = $derived($tableStore.columns);
  let result = $derived($tableStore.sqlResult);
  let charts = $derived($workspaceStore.doc?.charts ?? []);
  let dark = $derived($settings.darkMode);
  let theme = $derived($settings.theme);

  let type = $state<ChartType>('bar');
  let xCol = $state('');
  let yCol = $state('');
  let seriesCol = $state('');
  let chartName = $state('');
  let editingId = $state<string | null>(null);

  let el: HTMLDivElement | null = $state(null);
  let chart: EChartsType | null = null;
  let ready = $state(false);
  let prevCols: ColumnInfo[] | null = null;
  let skipDefaultsFor: string | null = null;

  function isNumericDtype(dtype: string): boolean {
    return /int|uint|float|double|decimal|hugeint/i.test(dtype);
  }

  function cellValue(row: { values: (string | number | boolean | null)[] }, idx: number): string {
    const v = row.values[idx];
    if (v === null || v === undefined) return '—';
    return String(v);
  }

  function cellNumber(row: { values: (string | number | boolean | null)[] }, idx: number): number | null {
    const v = row.values[idx];
    const n = typeof v === 'number' ? v : Number(v);
    return Number.isFinite(n) ? n : null;
  }

  function axisStyle(color: string, border: string): Record<string, unknown> {
    return {
      axisLine: { lineStyle: { color: border } },
      axisTick: { lineStyle: { color: border } },
      axisLabel: { color, fontSize: 11 },
      splitLine: { lineStyle: { color: border, type: 'dashed' } },
    };
  }

  function pad(data: (number | null)[], n: number): (number | null)[] {
    return Array.from({ length: n }, (_, i) => data[i] ?? null);
  }

  function buildOption(): Record<string, unknown> {
    const rows = result?.rows ?? [];
    const css = getComputedStyle(document.documentElement);
    const text = css.getPropertyValue('--text-primary').trim() || '#333';
    const sub = css.getPropertyValue('--text-secondary').trim() || '#888';
    const border = css.getPropertyValue('--border-color').trim() || '#e0e0e0';
    const xi = columns.findIndex(c => c.name === xCol);
    const yi = columns.findIndex(c => c.name === yCol);
    const si = seriesCol ? columns.findIndex(c => c.name === seriesCol) : -1;

    const base: Record<string, unknown> = {
      backgroundColor: 'transparent',
      animationDuration: 250,
      textStyle: { color: text },
      tooltip: { trigger: type === 'scatter' || type === 'pie' ? 'item' : 'axis' },
      grid: { left: 56, right: 24, top: 36, bottom: 36, containLabel: true },
    };

    if (type === 'pie') {
      return {
        ...base,
        legend: { type: 'scroll', top: 0, textStyle: { color: sub } },
        series: [{
          type: 'pie',
          radius: ['30%', '68%'],
          label: { color: text },
          data: rows.map(r => ({ name: cellValue(r, xi), value: cellNumber(r, yi) ?? 0 })),
        }],
      };
    }

    if (type === 'scatter') {
      return {
        ...base,
        xAxis: { type: 'value', name: xCol, nameTextStyle: { color: sub }, ...axisStyle(sub, border) },
        yAxis: { type: 'value', name: yCol, nameTextStyle: { color: sub }, ...axisStyle(sub, border) },
        series: [{ type: 'scatter', symbolSize: 8, data: rows.map(r => [cellNumber(r, xi), cellNumber(r, yi)]) }],
      };
    }

    // bar / line / area over a category axis, optionally split into series
    const cats: string[] = [];
    const catIdx = new Map<string, number>();
    const seriesMap = new Map<string, (number | null)[]>();
    for (const r of rows) {
      const cat = cellValue(r, xi);
      if (!catIdx.has(cat)) {
        catIdx.set(cat, cats.length);
        cats.push(cat);
      }
      const key = si >= 0 ? cellValue(r, si) : yCol;
      let arr = seriesMap.get(key);
      if (!arr) {
        arr = [];
        seriesMap.set(key, arr);
      }
      arr[catIdx.get(cat)!] = cellNumber(r, yi);
    }

    return {
      ...base,
      legend: si >= 0 ? { type: 'scroll', top: 0, textStyle: { color: sub } } : undefined,
      xAxis: { type: 'category', data: cats, ...axisStyle(sub, border) },
      yAxis: { type: 'value', ...axisStyle(sub, border) },
      series: [...seriesMap.entries()].map(([name, data]) => ({
        name,
        type: type === 'area' ? 'line' : type,
        smooth: type !== 'bar',
        showSymbol: type !== 'bar',
        areaStyle: type === 'area' ? {} : undefined,
        data: pad(data, cats.length),
      })),
    };
  }

  // New query result → sensible defaults for the mapping.
  $effect(() => {
    const cols = columns;
    if (cols === prevCols) return;
    prevCols = cols;
    if (!cols.length) return;
    if (skipDefaultsFor) {
      skipDefaultsFor = null;
      return;
    }
    const numeric = cols.filter(c => isNumericDtype(c.dtype));
    const x = cols.find(c => !isNumericDtype(c.dtype)) ?? cols[0];
    const y = numeric.find(c => c.name !== x.name) ?? cols.find(c => c.name !== x.name) ?? cols[0];
    xCol = x.name;
    yCol = y.name;
    seriesCol = '';
    editingId = null;
    chartName = `Chart ${charts.length + 1}`;
  });

  $effect(() => {
    void result; void columns; void xCol; void yCol; void seriesCol; void type;
    void dark; void theme; void ready;
    if (!chart) return;
    if (!result || !xCol || !yCol) {
      chart.clear();
      return;
    }
    chart.setOption(buildOption() as unknown as EChartsCoreOption, true);
  });

  onMount(() => {
    if (!el) return;
    chart = echarts.init(el);
    ready = true;
    const ro = new ResizeObserver(() => chart?.resize());
    ro.observe(el);
    return () => {
      ro.disconnect();
      chart?.dispose();
      chart = null;
      ready = false;
    };
  });

  async function openChart(c: ChartConfig) {
    if (c.sql.trim()) {
      try {
        const r = await executeSql(c.sql);
        tableStore.applyQueryResult(r, c.sql);
      } catch (e) {
        notify(`Chart query failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        return;
      }
    }
    skipDefaultsFor = c.id;
    type = c.type;
    xCol = c.x;
    yCol = c.y;
    seriesCol = c.series ?? '';
    chartName = c.name;
    editingId = c.id;
  }

  async function saveChart() {
    if (!result || !xCol || !yCol) {
      notify('Run a query first — the chart builder uses the latest query result.', 'error');
      return;
    }
    const name = chartName.trim() || `Chart ${charts.length + 1}`;
    const config: ChartConfig = {
      id: editingId ?? crypto.randomUUID(),
      name,
      sql: $tableStore.lastSql ?? '',
      type,
      x: xCol,
      y: yCol,
      series: seriesCol || null,
      updated_at: Date.now(),
    };
    chartName = name;
    editingId = config.id;
    await saveChartFlow(config);
  }

  function removeChart(c: ChartConfig) {
    if (editingId === c.id) editingId = null;
    void deleteChartFlow(c.id);
  }
</script>

<div class="charts-layout">
  <aside class="charts-side">
    <div class="side-head">Saved charts <span class="side-count">{charts.length}</span></div>
    {#if charts.length === 0}
      <div class="side-hint">
        Build a chart from a query result, then save it — charts travel with
        your <code>.parasql</code> workspace.
      </div>
    {/if}
    {#each charts as c (c.id)}
      <div class="chart-item" class:active={c.id === editingId}>
        <button class="chart-open" title={c.sql} onclick={() => void openChart(c)}>
          <span class="chart-name">{c.name}</span>
          <span class="chip">{c.type}</span>
        </button>
        <button
          class="chart-del"
          aria-label={`Delete chart ${c.name}`}
          title="Delete chart"
          onclick={() => removeChart(c)}
        >✕</button>
      </div>
    {/each}
  </aside>

  <section class="charts-main">
    {#if !result}
      <div class="hint">
        Run a query in the Query tab first — the chart builder visualizes the
        latest query result. Charts you save are stored in the workspace file.
      </div>
    {:else}
      <div class="controls">
        <label class="field name-field">
          <span>Name</span>
          <input bind:value={chartName} placeholder="Chart name" />
        </label>
        <label class="field">
          <span>Type</span>
          <select bind:value={type}>
            {#each CHART_TYPES as t (t.value)}
              <option value={t.value}>{t.label}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span>X / category</span>
          <select bind:value={xCol}>
            {#each columns as c (c.name)}
              <option value={c.name}>{c.name}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span>Y / value</span>
          <select bind:value={yCol}>
            {#each columns as c (c.name)}
              <option value={c.name}>{c.name}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span>Split by</span>
          <select bind:value={seriesCol}>
            <option value="">(none)</option>
            {#each columns as c (c.name)}
              <option value={c.name}>{c.name}</option>
            {/each}
          </select>
        </label>
        <button class="save-btn" onclick={() => void saveChart()}>Save chart</button>
      </div>
      <div class="chart-canvas" bind:this={el} role="img" aria-label={chartName || 'Chart preview'}></div>
    {/if}
  </section>
</div>

<style>
  .charts-layout {
    flex: 1;
    display: flex;
    min-height: 0;
    min-width: 0;
  }

  .charts-side {
    width: 220px;
    flex-shrink: 0;
    border-right: 1px solid var(--border-color, #e0e0e0);
    background: var(--panel-bg, #fafafa);
    overflow-y: auto;
    padding: 8px 0;
  }

  .side-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary, #888);
  }

  .side-count {
    font-weight: 500;
  }

  .side-hint {
    padding: 4px 12px 10px;
    font-size: 11px;
    line-height: 1.6;
    color: var(--text-secondary, #888);
  }

  .side-hint code {
    background: var(--code-bg, #f5f5f5);
    padding: 0 3px;
    border-radius: 3px;
  }

  .chart-item {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 6px 0 12px;
  }

  .chart-item.active {
    background: var(--selected-bg, #e8f0fe);
  }

  .chart-open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 0;
    border: none;
    background: none;
    cursor: pointer;
    font-size: 12px;
    color: var(--text-primary, #333);
    text-align: left;
  }

  .chart-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip {
    flex-shrink: 0;
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 8px;
    background: var(--hover-bg, #f5f5f5);
    color: var(--text-secondary, #888);
  }

  .chart-del {
    flex-shrink: 0;
    border: none;
    background: none;
    color: var(--text-secondary, #888);
    cursor: pointer;
    font-size: 11px;
    padding: 4px;
    border-radius: 4px;
  }

  .chart-del:hover {
    color: var(--error-text, #991b1b);
    background: var(--hover-bg, #f5f5f5);
  }

  .charts-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    padding: 8px;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px;
    padding: 6px 4px 10px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11px;
    color: var(--text-secondary, #888);
  }

  .field input,
  .field select {
    font-size: 12px;
    padding: 4px 6px;
    border: 1px solid var(--border-color, #e0e0e0);
    border-radius: 4px;
    background: var(--bg, #fff);
    color: var(--text-primary, #333);
  }

  .name-field input {
    width: 160px;
  }

  .save-btn {
    padding: 5px 14px;
    border: none;
    border-radius: 4px;
    background: var(--accent-color, #1a73e8);
    color: var(--accent-text, #fff);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .save-btn:hover {
    background: var(--accent-hover, #1557b0);
  }

  .chart-canvas {
    flex: 1;
    min-height: 260px;
    min-width: 0;
  }

  .hint {
    margin: 24px 16px;
    padding: 14px 16px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-secondary, #888);
    border: 1px dashed var(--border-color, #d0d0d0);
    border-radius: 6px;
  }
</style>
