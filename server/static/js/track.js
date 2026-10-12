/* First-party visit analytics: page views, clicks (for the heat map), scroll depth, searches,
   experiment exposures and conversions, posted in small batches to /api/t on this server. Never
   loaded for the signed-in admin. Does nothing when the browser sends Do Not Track or Global
   Privacy Control, or when the server gave no visitor cookie. No third party is involved. */
(function () {
  "use strict";
  if (navigator.doNotTrack === "1" || window.doNotTrack === "1" || navigator.globalPrivacyControl) return;
  if (!/(?:^|;\s*)pm_v=[0-9a-f]{24}/.test(document.cookie)) return;

  var path = location.pathname, queue = [], timer = null;
  var vw = function () { return window.innerWidth || document.documentElement.clientWidth || 0; };
  var device = vw() < 600 ? "phone" : (vw() < 1024 ? "tablet" : "desktop");
  var art = document.querySelector("[data-product-id]");
  var pid = art ? parseInt(art.getAttribute("data-product-id"), 10) : null;
  var docH = function () { return Math.max(document.documentElement.scrollHeight, document.body.scrollHeight, 1); };
  var docW = function () { return Math.max(document.documentElement.scrollWidth, 1); };

  function flush(last) {
    if (timer) { clearTimeout(timer); timer = null; }
    if (!queue.length) return;
    var body = JSON.stringify({ p: path, d: device, pid: isNaN(pid) ? null : pid, e: queue.splice(0, 40) });
    try {
      if (navigator.sendBeacon && navigator.sendBeacon("/api/t", new Blob([body], { type: "application/json" }))) return;
      fetch("/api/t", { method: "POST", body: body, headers: { "Content-Type": "application/json" }, credentials: "same-origin", keepalive: !!last });
    } catch (e) { /* analytics never breaks the page */ }
  }
  function push(kind, detail) { queue.push({ k: kind, d: detail || {} }); if (queue.length >= 20) flush(); else if (!timer) timer = setTimeout(flush, 4000); }
  window.pmTrack = push; // guide.js reports the questions people type

  // ---- view ------------------------------------------------------------------------------------
  var ref = "";
  try { if (document.referrer) { var u = new URL(document.referrer); if (u.host !== location.host) ref = u.host; } } catch (e) { }
  push("view", { ref: ref, dh: docH(), vw: vw() });

  // ---- experiments this page is exposing (body[data-exp="key:variant,..."]), once per visitor ------
  (document.body.getAttribute("data-exp") || "").split(",").forEach(function (pair) {
    var kv = pair.split(":"); if (kv.length !== 2 || !kv[0]) return;
    var flag = "pm-exp-" + kv[0] + "-" + kv[1];
    try { if (localStorage.getItem(flag)) return; localStorage.setItem(flag, "1"); } catch (e) { }
    push("exposure", { exp: kv[0], "var": kv[1] });
  });

  // ---- search: the shop prints its result count next to the words searched ----------------------
  var rc = document.querySelector("[data-results-total]");
  if (rc && rc.getAttribute("data-results-q") && !/[?&]page=/.test(location.search)) {
    push("search", { q: rc.getAttribute("data-results-q"), n: parseInt(rc.getAttribute("data-results-total"), 10) || 0, source: "shop" });
  }

  // ---- clicks ----------------------------------------------------------------------------------
  function selectorOf(el) {
    var parts = [];
    while (el && el.nodeType === 1 && el !== document.body && parts.length < 8) {
      if (el.id) { parts.unshift("#" + el.id); break; }
      var tag = el.tagName.toLowerCase(), i = 1, sib = el;
      while ((sib = sib.previousElementSibling)) if (sib.tagName === el.tagName) i++;
      parts.unshift(tag + ":nth-of-type(" + i + ")");
      el = el.parentElement;
    }
    return parts.join(" > ");
  }
  var LIVE = "a,button,input,select,textarea,label,summary,details,canvas,video,[role=button],[data-edit],[tabindex]";
  document.addEventListener("click", function (e) {
    var el = e.target; if (!el || el.nodeType !== 1) return;
    var live = el.closest(LIVE), box = (live || el), r = box.getBoundingClientRect();
    var link = el.closest("a[href]"), href = link ? link.getAttribute("href") : "";
    push("click", {
      sel: selectorOf(box), tx: (box.innerText || box.getAttribute("aria-label") || box.getAttribute("alt") || "").replace(/\s+/g, " ").trim().slice(0, 60), href: href || "",
      rx: r.width ? +((e.clientX - r.left) / r.width).toFixed(3) : 0.5, ry: r.height ? +((e.clientY - r.top) / r.height).toFixed(3) : 0.5,
      px: +(e.pageX / docW()).toFixed(4), py: +(e.pageY / docH()).toFixed(4), dead: !live, vw: vw()
    });
    if (el.closest(".demo-buy summary")) push("conversion", { type: "buy" });
    else if (/^tel:/.test(href)) push("conversion", { type: "call" });
    else if (/^mailto:/.test(href)) push("conversion", { type: "email" });
    if (link && !/^(#|javascript:)/.test(href)) flush(true);
  }, true);
  document.addEventListener("submit", function (e) {
    var f = e.target, a = (f.getAttribute && f.getAttribute("action")) || "";
    if (/(pen-repairs|sell-my-pens|contact|post-your-product|mailing-list)/.test(a) && (f.method || "").toLowerCase() === "post") { push("conversion", { type: "form:" + a.replace(/\//g, "") }); flush(true); }
    else if (/^\/checkout\/[^/]+\/$/.test(a)) { push("conversion", { type: "buy" }); flush(true); }
  }, true);

  // ---- scroll depth, sent once when the page is left ----------------------------------------------
  var maxDepth = 0, sent = false;
  function depth() { var d = Math.min(100, Math.round(((window.scrollY || window.pageYOffset || 0) + window.innerHeight) / docH() * 100)); if (d > maxDepth) maxDepth = d; }
  window.addEventListener("scroll", depth, { passive: true }); depth();
  function leave() { if (!sent) { sent = true; push("scroll", { max: maxDepth }); } flush(true); }
  document.addEventListener("visibilitychange", function () { if (document.visibilityState === "hidden") leave(); });
  window.addEventListener("pagehide", leave);
})();
