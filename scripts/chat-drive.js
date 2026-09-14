// Drive two real chat turns on the deployed page over CDP and screenshot them.
//   chromium --headless=new --remote-debugging-port=9333 about:blank &   then   node chat-drive.js <out-dir> <base-url>
// Node 22+ (built-in WebSocket), no deps. Reads the answers: the engine must have been called (a .gcard
// appears) and the reply must be plain text.
const base = (process.argv[3] || "http://127.0.0.1:8140").replace(/\/$/, ""); const out = process.argv[2] || ".";
const sleep = (ms) => new Promise(r => setTimeout(r, ms));
const fs = require("fs");
(async () => {
  const list = await (await fetch("http://127.0.0.1:9333/json")).json();
  const page = list.find(p => p.type === "page");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise(r => ws.onopen = r);
  let id = 0; const pending = {};
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending[m.id]) { pending[m.id](m); delete pending[m.id]; } };
  const send = (method, params = {}) => new Promise(res => { const i = ++id; pending[i] = res; ws.send(JSON.stringify({ id: i, method, params })); });
  const shot = async (name) => { const r = await send("Page.captureScreenshot", { format: "png" }); fs.writeFileSync(`${out}/${name}.png`, Buffer.from(r.result.data, "base64")); console.log("shot", name); };
  const evalJs = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
  await send("Page.enable"); await send("Runtime.enable");
  const ask = async (q, name) => {
    await evalJs(`(()=>{const i=document.querySelector('#chatIn'); i.value=${JSON.stringify(q)}; document.querySelector('#chatSend').click(); return 'sent'})()`);
    let ok = false;
    for (let i = 0; i < 60; i++) { await sleep(1000); const n = await evalJs(`document.querySelectorAll('#chat .msg.ai').length`); const busy = await evalJs(`!!document.querySelector('#chat .typing')`); if (n >= 2 && !busy) { ok = true; break; } }
    await sleep(600);
    await evalJs(`document.querySelector('#guidePhone').scrollIntoView({block:'center'}); document.querySelector('#chat').scrollTop = 1e6`); await sleep(400);
    const cards = await evalJs(`document.querySelectorAll('#chat .gcard').length`);
    const reply = await evalJs(`[...document.querySelectorAll('#chat .msg.ai')].pop()?.textContent || ''`);
    console.log(`Q: ${q}\nA: ${reply}\ncards: ${cards} ok: ${ok}`);
    await shot(name);
    return { ok, cards, reply };
  };
  await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 1000, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: base + "/#ask" }); await sleep(4500);
  await shot("1-home-desktop");
  const a = await ask("I want a flexible nib vintage pen under $300. What do you have?", "2-guide-live-turn-desktop");
  await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
  await send("Page.navigate", { url: base + "/?x=2#ask" }); await sleep(4500);
  await shot("3-home-phone");
  const b = await ask("Can you fix my grandfather's Sheaffer Snorkel?", "4-guide-live-turn-phone");
  ws.close();
  const pass = a.ok && a.cards > 0 && b.ok && b.cards > 0 && !/[*#]/.test(a.reply + b.reply);
  console.log(pass ? "chat-drive PASS" : "chat-drive FAIL");
  process.exit(pass ? 0 : 1);
})().catch(e => { console.error(e); process.exit(1); });
