// Read the current tableStore state from the live app (no reload).
import { connect, getPage, disconnect } from './lib.mjs';

await connect({ reload: false });
const v = await getPage().evaluate(async () => {
  const m = await import('/src/lib/stores/table.ts');
  let s;
  m.tableStore.subscribe((x) => (s = x))();
  return { filePath: s.filePath, modified: s.modified, totalRows: s.totalRows, rows: s.rows.length };
});
console.log(JSON.stringify(v));
await disconnect();
