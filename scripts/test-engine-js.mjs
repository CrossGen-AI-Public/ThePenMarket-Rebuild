// Run the shared engine fixtures against the JavaScript mirror. Same cases as `cargo test`.
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const E = require(path.join(here, "..", "server", "static", "js", "engine.js"));
const fx = JSON.parse(readFileSync(path.join(here, "..", "server", "tests", "fixtures", "engine-cases.json"), "utf8"));
const catalog = { pens: fx.catalog };
let failed = 0;
const eq = (name, got, want) => { if (JSON.stringify(got) !== JSON.stringify(want)) { failed++; console.log(`FAIL ${name}: got ${JSON.stringify(got)} want ${JSON.stringify(want)}`); } };
for (const c of fx.cases) {
  const x = c.expect;
  switch (c.tool) {
    case "find_pens": { const r = E.findPens(catalog, c.args); if (x.total !== undefined) eq(c.name + " total", r.total_matching, x.total); if (x.top_skus) eq(c.name + " order", r.matches.map(m => m.sku), x.top_skus); if (x.note_contains && !r.note.includes(x.note_contains)) { failed++; console.log(`FAIL ${c.name}: note '${r.note}'`); } break; }
    case "pen_details": { const p = E.penDetails(catalog, c.args.key); eq(c.name, p ? p.sku : null, x.sku ?? null); break; }
    case "repair_scope": eq(c.name, E.repairScope(c.args.text).verdict, x.verdict); break;
    case "sell_triage": eq(c.name, E.sellTriage(c.args.text).verdict, x.verdict); break;
    case "guarantee_dates": { const r = E.guaranteeDates(c.args.received_on); eq(c.name + " return", r.return_by, x.return_by); eq(c.name + " fix", r.fix_by, x.fix_by); break; }
    case "stats": { const s = E.stats(catalog); eq(c.name + " live", s.live_total, x.live_total); eq(c.name + " grails", s.grail_count, x.grail_count); eq(c.name + " first", s.first_pen_count, x.first_pen_count); break; }
    case "nib_fact": { const f = E.nibFact(c.args.name); eq(c.name, f ? f.slug : null, x.slug ?? null); break; }
    default: failed++; console.log("unknown tool " + c.tool);
  }
}
console.log(failed ? `engine.js: ${failed} failed` : `engine.js: all ${fx.cases.length} fixture cases pass`);
process.exit(failed ? 1 : 0);
