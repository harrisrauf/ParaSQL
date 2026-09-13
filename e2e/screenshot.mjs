// Capture README screenshots from the running app.
// Usage: npm run dev:e2e (in another terminal), then: node e2e/screenshot.mjs
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { connect, openWorkspace, runSql, ensureQueryTab, getPage, disconnect, WS_DEMO } from './lib.mjs';

const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'docs', 'assets');

await connect({ reload: true });
await openWorkspace(WS_DEMO, { direct: true }).catch(() => {});
await ensureQueryTab();
await runSql(
  `SELECT p.category, SUM(oi.quantity * oi.unit_price) AS revenue, COUNT(*) AS orders
   FROM order_items oi
   JOIN orders o ON o.order_id = oi.order_id
   JOIN products p ON p.product_id = oi.product_id
   GROUP BY 1 ORDER BY 2 DESC;`,
  { timeout: 60000 }
);
await new Promise((r) => setTimeout(r, 800));
const target = path.join(OUT, 'screenshot-query.png');
await getPage().screenshot({ path: target });
console.log('wrote', target);
await disconnect();
process.exit(0);
