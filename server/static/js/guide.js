/* Ask the Pen Market. A real model talks; PEN_ENGINE (page) or server/src/engine.rs (server) does
   the numbers. Two modes, picked at load: claude.ai artifact (`sample`, tools run in the page against
   the inlined catalog snapshot) or the server API (/api/guide, tools run in Rust). No model reachable:
   the assistant says it is offline and shows the human paths. Never a canned answer. */
(function () {
  "use strict";
  var E = window.PEN_ENGINE;
  var state = { turns: [], mode: "offline", sample: null, busy: false, api: "", backend: null, catalog: window.PEN_CATALOG || null };
  var $ = function (s, r) { return (r || document).querySelector(s); };
  var chatEl, inputEl, sendEl, quickEl;

  var PHONE = "(847) 708-5062", EMAIL = "info@thepenmarket.com";

  function systemPrompt() {
    var s = state.catalog && E ? E.stats(state.catalog) : null;
    var cats = s ? s.by_category.map(function (c) { return c.count + " " + c.name; }).join(", ") : "";
    var line = s ? "The catalog right now: " + s.live_total + " items in stock (" + cats + "), " + E.fmtDollars(s.price_min_cents) + " to " + E.fmtDollars(s.price_max_cents) + ", " + s.brands_with_stock + " brands with stock. Never repeat these counts from memory later; call the tools." : "";
    return "You are Ask the Pen Market, the AI guide on ThePenMarket.com, Nathaniel Cerf's vintage pen shop (online since 2007, Norwich, Connecticut). You are an AI, not Nathaniel; say so if asked. Nathaniel restores and sells vintage fountain pens and pre-owned luxury pens, one of each, repairs vintage pens, buys and consigns collections, and runs the Trading Post classifieds ($5 a listing, $125 a year for Unlimited Posts). The blog is Drippy Musings.\nVoice: plain, warm, a little wry, collector to collector, like his blog. Short messages: at most three short sentences or a short list. One question at a time. Plain text only: no markdown, no asterisks, no headings, no emojis, no em dashes.\n" + line + "\nRULES:\n- NEVER state a price, count, length, date, availability or whether a repair is in scope from memory. Only report what your tools return. If you do not have a tool result yet, ask for the missing detail or call the tool.\n- To find pens, call find_pens as soon as you have any one of: a budget, a brand, an era, a nib, a filling system, a category, or a few keywords. Do not ask more than one clarifying question before searching. Parse sensibly: \"under 200\" is max_price 200; \"1930s\" is era 1930s; \"flex\" is nib flex.\n- After find_pens, name the top one or two pens with their price and one reason, mention how many matched, and offer to narrow. The page shows cards for the results; do not list every field.\n- For a specific pen (a SKU, a model name), call pen_details.\n- For \"can you fix my X\" call repair_scope with the visitor's words. For \"do you buy X\" or \"I want to sell\" call sell_triage. For nib width questions call nib_fact. For return or guarantee dates call guarantee_dates with the delivery date.\n- Estimates for repairs and offers for collections come from Nathaniel, never from you. Do not invent turnaround times, repair prices or offers.\n- Authenticity: you cannot authenticate a pen. Point to his fake-Montblanc posts and the description on the listing.\n- Checkout is not connected in this preview build; say a visitor can contact the shop to buy, and never pretend to take payment.\n- Off topic: one sentence, then back to pens.\n- When the visitor wants a repair estimate, wants to sell, wants a pen over $1,000, or asks for a person, call handoff with a two-sentence summary; then give the phone " + PHONE + " and " + EMAIL + ".\n- HARD RULE: if the visitor's first message already contains a budget or a pen description, your first action is find_pens, not a question.";
  }

  // Tools for artifact mode (run in the page). The server has the same seven.
  var tools = [
    { name: "find_pens", description: "Search the live catalog and rank matching pens with prices and reasons. Every argument is optional.", inputSchema: { type: "object", properties: { max_price: { type: "number" }, min_price: { type: "number" }, category: { type: "string" }, brand: { type: "string" }, era: { type: "string" }, nib: { type: "string" }, mechanism: { type: "string" }, flex: { type: "boolean" }, keywords: { type: "string" }, include_sold: { type: "boolean" }, limit: { type: "integer" } }, required: [] },
      execute: function (i) { var r = E.findPens(state.catalog, i || {}); renderFind(r); return r; } },
    { name: "pen_details", description: "Full details of one pen by SKU, slug or name.", inputSchema: { type: "object", properties: { key: { type: "string" } }, required: ["key"] },
      execute: function (i) { var p = E.penDetails(state.catalog, i && i.key); if (!p) return { error: "no pen matches that key; ask for the SKU or the model name as it appears on the site" }; var price = p.sale_price_cents != null ? p.sale_price_cents : p.price_cents; var m = { sku: p.sku, title: p.title, short_title: p.short_title, url: "/product/" + p.slug + "/", image: p.image, price: E.fmtCents(price), was_price: p.sale_price_cents != null ? E.fmtCents(p.price_cents) : null, brand: p.brand, era: p.era, nib: p.nib, mechanism: p.mechanism, status: p.status, reasons: [], summary: p.summary }; renderFind({ matches: [m], total_matching: 1, shown: 1, note: "" }); return Object.assign({ length_cm: p.length_mm != null ? (p.length_mm / 10).toFixed(1) : null, repairable_system: p.repairable }, m); } },
    { name: "repair_scope", description: "Whether a described pen or filling system is on his published repair list.", inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
      execute: function (i) { var r = E.repairScope(i && i.text || ""); renderRepair(r); return r; } },
    { name: "sell_triage", description: "Whether he is looking for the pens a visitor wants to sell, and the options.", inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
      execute: function (i) { var r = E.sellTriage(i && i.text || ""); renderSell(r); return r; } },
    { name: "guarantee_dates", description: "Return-by and repair-promise dates from the day a pen was received (YYYY-MM-DD).", inputSchema: { type: "object", properties: { received_on: { type: "string" } }, required: ["received_on"] },
      execute: function (i) { if (!i || !/^\d{4}-\d{2}-\d{2}$/.test(i.received_on || "")) return { error: "received_on must be a date like 2026-09-14; ask the visitor when the pen arrived" }; var r = E.guaranteeDates(i.received_on); renderDates(r); return r; } },
    { name: "nib_fact", description: "Line width and notes for a nib grade.", inputSchema: { type: "object", properties: { name: { type: "string" } }, required: ["name"] },
      execute: function (i) { return E.nibFact(i && i.name || "") || { error: "unknown nib grade; the grades are extra-fine, fine, medium, broad, BB, stub, oblique, accountant, semi-flexible, flexible" }; } },
    { name: "handoff", description: "Record a summary for Nathaniel and show the visitor the human contact card.", inputSchema: { type: "object", properties: { summary: { type: "string" } }, required: ["summary"] },
      execute: function (i) { renderHandoff(i && i.summary || ""); return { ok: true, phone: PHONE, email: EMAIL }; } }
  ];

  // ---------- DOM ----------
  var pinRaf = 0, observer = null;
  function pinToEnd() { if (!chatEl) return; cancelAnimationFrame(pinRaf); pinRaf = requestAnimationFrame(function () { chatEl.scrollTop = chatEl.scrollHeight; }); }
  var esc = function (s) { return String(s == null ? "" : s).replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; }); };
  var plain = function (t) { return String(t || "").replace(/\*\*(.+?)\*\*/g, "$1").replace(/__(.+?)__/g, "$1").replace(/^\s*[*-]\s+/gm, "• ").replace(/^\s*#{1,6}\s+/gm, "").replace(/`([^`]+)`/g, "$1").replace(/\n{3,}/g, "\n\n").trim(); };
  function bubble(kind, text) { var d = document.createElement("div"); d.className = "msg " + kind; d.textContent = kind === "ai" ? plain(text) : text; chatEl.appendChild(d); pinToEnd(); return d; }
  function addCard(html) { var d = document.createElement("div"); d.innerHTML = html; while (d.firstChild) chatEl.appendChild(d.firstChild); pinToEnd(); }
  function link(href) { return (window.PEN_SITE_BASE || "") + href; }

  function renderFind(r) {
    if (!r || !r.matches || !r.matches.length) { if (r && r.note) addCard('<div class="gcard no"><div class="t"><span>No match</span></div><p>' + esc(r.note) + '</p></div>'); return; }
    var rows = r.matches.slice(0, 5).map(function (m) {
      return '<a class="pen" href="' + esc(link(m.url)) + '">' + (m.image ? '<img src="' + esc(m.image) + '" alt="">' : '<span></span>') + '<span><strong>' + esc(m.short_title) + '</strong><small>' + esc((m.reasons || [])[0] || m.era || "") + (m.status === "sold" ? " · SOLD" : "") + '</small></span><b>' + esc(m.price) + '</b></a>';
    }).join("");
    addCard('<div class="gcard ok"><div class="t"><span>' + (r.total_matching === 1 ? "1 pen" : r.total_matching + " pens") + ' in the case</span><span class="mono small">from the catalog</span></div>' + rows + (r.total_matching > r.shown ? '<p class="small muted">Showing ' + r.shown + ' of ' + r.total_matching + '.</p>' : '') + '</div>');
  }
  function renderRepair(r) {
    var ok = r.verdict === "in_scope";
    addCard('<div class="gcard ' + (ok ? "ok" : r.verdict === "out_of_scope" ? "no" : "") + '"><div class="t"><span>' + (ok ? "On his repair list" : r.verdict === "out_of_scope" ? "Not restored here" : "Need the filling system") + '</span></div><p>' + esc(r.message) + '</p><ul>' + (ok ? '<li>' + esc(r.discount_note) + '</li><li>Ship it: ' + esc((r.shipping_steps || []).join(" ")) + '</li>' : '') + '<li>' + esc(r.next_step) + '</li></ul><p><a class="btn btn-navy btn-sm" href="' + esc(link("/pen-repairs/")) + '">Free repair estimate</a></p></div>');
  }
  function renderSell(r) {
    addCard('<div class="gcard ' + (r.verdict === "interested" ? "ok" : r.verdict === "not_looking" ? "no" : "") + '"><div class="t"><span>' + (r.verdict === "interested" ? "He is usually looking for these" : r.verdict === "not_looking" ? "Not on his list" : "Tell it what you have") + '</span></div><p>' + esc(r.message) + '</p><ul><li>Options: ' + esc((r.options || []).join(" or ")) + '</li><li>' + esc(r.next_step) + '</li></ul><p><a class="btn btn-navy btn-sm" href="' + esc(link("/sell-my-pens/")) + '">Sell My Pens form</a> <a class="btn btn-ghost btn-sm" href="' + esc(link("/trading-post/")) + '">Trading Post</a></p></div>');
  }
  function renderDates(r) {
    addCard('<div class="gcard ok"><div class="t"><span>Our Guarantee dates</span><span class="mono small">received ' + esc(r.received_on) + '</span></div><ul><li>Return by ' + esc(r.return_by) + ' (' + r.return_days + ' days) for a full refund of the item price, same condition, insured U.S. Mail</li><li>Problems found by ' + esc(r.fix_by) + ' (' + r.fix_days + ' days): let him know and he will do what he can to fix it</li></ul></div>');
  }
  function renderHandoff(summary) {
    addCard('<div class="gcard ok"><div class="t"><span>Over to Nathaniel</span><span class="mono small">Norwich, CT</span></div>' + (summary ? '<p>' + esc(summary) + '</p>' : '') + '<ul><li>Call <a href="tel:+18477085062">' + PHONE + '</a></li><li>E-mail <a href="mailto:' + EMAIL + '">' + EMAIL + '</a></li><li>Estimates and offers come from him, not from this guide</li></ul></div>');
    quickEl.innerHTML = "";
  }
  function renderToolResults(list) {
    (list || []).forEach(function (t) {
      if (!t || !t.result) return;
      if (t.name === "find_pens" || t.name === "pen_details") renderFind(t.result);
      else if (t.name === "repair_scope") renderRepair(t.result);
      else if (t.name === "sell_triage") renderSell(t.result);
      else if (t.name === "guarantee_dates") renderDates(t.result);
      else if (t.name === "handoff") renderHandoff(t.result.summary);
    });
  }
  function setQuick(items) { quickEl.innerHTML = items.map(function (t) { return "<button type=\"button\">" + esc(t) + "</button>"; }).join(""); }

  function probe(base) { return fetch(base + "/api/guide/health", { cache: "no-store" }).then(function (r) { return r.ok ? r.json() : null; }).then(function (j) { return j && j.backend && j.backend !== "none" ? j : null; }).catch(function () { return null; }); }
  function detectMode() {
    var p = Promise.resolve(null);
    if (window.claude && window.claude.use && state.catalog && E) {
      p = Promise.resolve().then(function () { return claude.use("sample"); }).then(function (s) { if (s) { state.sample = s; return "artifact"; } return null; }).catch(function () { return null; });
    }
    return p.then(function (m) { if (m) return m; if (window.PEN_NO_SERVER) return "offline"; return probe("").then(function (j) { if (j) { state.api = ""; state.backend = j; return "api"; } return "offline"; }); });
  }
  function setBadge() {
    var el = $("#guidePhone .who small"), dot = $("#guidePhone .dot");
    if (el) el.textContent = state.mode === "offline" ? "Assistant offline" : "AI assistant · catalog numbers only";
    if (dot) dot.style.background = state.mode === "offline" ? "#c33" : "#34c76a";
  }

  function start() {
    state.turns = []; chatEl.innerHTML = "";
    bubble("sys", "Ask the Pen Market is an AI assistant. Prices, counts and dates come from the catalog and the published policies, not from the model. Nathaniel makes every estimate and offer.");
    detectMode().then(function (mode) {
      state.mode = mode; setBadge();
      if (mode === "offline") { offline(); return; }
      var sku = (location.hash.match(/sku=([^&]+)/) || [])[1];
      bubble("ai", "Hello. Tell me what you want to write with, or what you have: a budget, a brand, an era, a nib, a filling system. I can also say whether a pen is on the repair list, whether he is buying what you have, and when a return window closes.");
      setQuick(sku ? ["Tell me about SKU " + decodeURIComponent(sku), "What else is like it?"] : ["A flexible nib under $200", "First fountain pen, under $150", "Can you fix my grandfather's Vacumatic?", "Do you buy Cross Century pens?"]);
      if (sku) send("Tell me about SKU " + decodeURIComponent(sku));
    });
  }

  function send(text) {
    text = (text || "").trim(); if (!text || state.busy) return;
    inputEl.value = ""; bubble("me", text); quickEl.innerHTML = "";
    if (state.mode === "offline") { offline(); return; }
    state.turns.push({ role: "user", content: text });
    state.busy = true; sendEl.disabled = true;
    var ai = bubble("ai", ""); ai.innerHTML = '<span class="typing"><i></i><i></i><i></i></span>';
    var done = function (reply) {
      ai.textContent = plain(reply); chatEl.appendChild(ai); pinToEnd();
      state.turns.push({ role: "assistant", content: reply });
      if (state.turns.length > 24) state.turns = state.turns.slice(-24);
      if (!/\(847\)/.test(reply)) setQuick(["Show me something similar", "What about under $150?", "Talk to Nathaniel"]);
      state.busy = false; sendEl.disabled = false;
    };
    var fail = function (msg) { state.turns.pop(); ai.textContent = msg; state.busy = false; sendEl.disabled = false; };
    if (state.mode === "artifact") {
      var input = [{ role: "user", content: systemPrompt() + "\n\n(Conversation begins. Greet only once; the greeting already happened.)" }, { role: "assistant", content: "Understood." }].concat(state.turns);
      state.sample(input, { tools: tools, cache: false, modelTier: "default", onText: function (ev) { ai.textContent = plain(ev.text); pinToEnd(); } })
        .then(function (res) { done((res && res.text) || ai.textContent || "Here is what the catalog returned."); })
        .catch(function (err) { if (err && err.code === "not_granted") { state.sample = null; state.mode = "offline"; setBadge(); offline(); } fail("I hit a snag reaching the model. Try again in a moment, or call " + PHONE + "."); });
    } else {
      fetch(state.api + "/api/guide", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ messages: state.turns }) })
        .then(function (r) { if (!r.ok) throw new Error("server " + r.status); return r.json(); })
        .then(function (j) { renderToolResults(j.toolResults); done(j.text || ((j.toolResults || []).length ? "Here is what the catalog returned." : "")); })
        .catch(function () { fail("I hit a snag reaching the model. Try again in a moment, or call " + PHONE + " / " + EMAIL + "."); });
    }
  }

  function offline() {
    bubble("ai", "The guide is offline right now, so it cannot run the catalog for you. Nathaniel can: call " + PHONE + " or e-mail " + EMAIL + ". The shop's own filters still work without me.");
    quickEl.innerHTML = "";
    addCard('<div class="gcard"><ul><li><a href="' + esc(link("/shop/")) + '">Browse the case with filters</a></li><li><a href="' + esc(link("/pen-repairs/")) + '">Free repair estimate</a></li><li><a href="' + esc(link("/sell-my-pens/")) + '">Sell my pens</a></li></ul></div>');
    inputEl.disabled = true; inputEl.placeholder = "Assistant offline"; sendEl.disabled = true;
  }

  function bind() {
    chatEl = $("#chat"); inputEl = $("#chatIn"); sendEl = $("#chatSend"); quickEl = $("#quick");
    if (!chatEl || !inputEl) return;
    chatEl.style.scrollBehavior = "auto";
    if (observer) observer.disconnect();
    observer = new MutationObserver(pinToEnd); observer.observe(chatEl, { childList: true, subtree: true, characterData: true });
    sendEl.onclick = function () { send(inputEl.value); };
    inputEl.onkeydown = function (e) { if (e.key === "Enter") send(inputEl.value); };
    quickEl.onclick = function (e) { var b = e.target.closest("button"); if (b) send(b.textContent); };
    start();
  }
  window.PEN_GUIDE = { bind: bind, start: start };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", bind); else bind();
})();
