/* The admin's heat map: press "Heat map" in the black bar on any page and the clicks visitors made
   on that page are drawn over it, with lines showing how far down people scrolled and a list of
   the clicks that went nowhere. Each click is stored relative to the thing that was clicked, so it
   lands in the right place whatever the width of this window. Signed-in admin only. */
(function () {
  "use strict";
  var btn = document.getElementById("admHeat"); if (!btn) return;
  var on = false, layer = null, panel = null, device = "", days = 30, data = null;

  function docSize() { var d = document.documentElement, b = document.body; return { w: Math.max(d.scrollWidth, b.scrollWidth), h: Math.max(d.scrollHeight, b.scrollHeight) }; }
  function place(c, size) {
    var el = null;
    if (c.sel) { try { el = document.querySelector(c.sel); } catch (e) { el = null; } }
    if (el) {
      var r = el.getBoundingClientRect();
      if (r.width || r.height) return { x: r.left + window.scrollX + r.width * (c.rx == null ? 0.5 : c.rx), y: r.top + window.scrollY + r.height * (c.ry == null ? 0.5 : c.ry) };
    }
    if (c.px == null || c.py == null) return null;
    return { x: c.px * size.w, y: c.py * size.h };
  }

  function draw() {
    if (layer) layer.remove();
    var size = docSize();
    layer = document.createElement("div"); layer.className = "heat-layer"; layer.style.height = size.h + "px";
    // Canvases have a height limit; long pages are drawn in slices.
    var SLICE = 4000;
    for (var top = 0; top < size.h; top += SLICE) {
      var cv = document.createElement("canvas"), hgt = Math.min(SLICE, size.h - top);
      cv.width = size.w; cv.height = hgt; cv.style.top = top + "px"; cv.style.width = size.w + "px"; cv.style.height = hgt + "px";
      layer.appendChild(cv);
    }
    var canvases = layer.querySelectorAll("canvas");
    (data.clicks || []).forEach(function (c) {
      var p = place(c, size); if (!p) return;
      for (var i = 0; i < canvases.length; i++) {
        var y0 = i * SLICE; if (p.y < y0 - 40 || p.y > y0 + SLICE + 40) continue;
        var ctx = canvases[i].getContext("2d"), g = ctx.createRadialGradient(p.x, p.y - y0, 0, p.x, p.y - y0, 34);
        // One hue, more ink where more clicks pile up; clicks that went nowhere are ringed, not recoloured.
        g.addColorStop(0, "rgba(237,28,36,.38)"); g.addColorStop(1, "rgba(237,28,36,0)");
        ctx.fillStyle = g; ctx.beginPath(); ctx.arc(p.x, p.y - y0, 34, 0, Math.PI * 2); ctx.fill();
        if (c.dead) { ctx.strokeStyle = "rgba(27,27,27,.75)"; ctx.lineWidth = 2; ctx.setLineDash([4, 3]); ctx.beginPath(); ctx.arc(p.x, p.y - y0, 9, 0, Math.PI * 2); ctx.stroke(); ctx.setLineDash([]); }
      }
    });
    (data.reach || []).forEach(function (r) {
      var line = document.createElement("div"); line.className = "heat-reach"; line.style.top = Math.min(size.h - 2, size.h * r.pct / 100) + "px";
      line.textContent = Math.round(r.share * 100) + "% of visitors scrolled this far";
      layer.appendChild(line);
    });
    document.body.appendChild(layer);
  }

  function summarise() {
    var dead = {}, clicks = data.clicks || [];
    clicks.forEach(function (c) { if (c.dead) { var k = c.tx || c.sel || "?"; dead[k] = (dead[k] || 0) + 1; } });
    var rows = Object.keys(dead).map(function (k) { return [k, dead[k]]; }).sort(function (a, b) { return b[1] - a[1]; }).slice(0, 6);
    var esc = function (t) { return String(t).replace(/[&<>"']/g, function (ch) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[ch]; }); };
    var html = '<b>Heat map: ' + esc((function () { try { return decodeURI(location.pathname); } catch (e) { return location.pathname; } })()) + '</b><p>' + esc(Number(data.views) || 0) + ' visitors, ' + clicks.length + ' clicks in the last ' + esc(days) + ' days' + (device ? ' on a ' + esc(device) : '') + '.</p>';
    html += '<label>Show <select data-dev><option value="">every device</option><option value="desktop">computers</option><option value="tablet">tablets</option><option value="phone">phones</option></select></label> <label>over <select data-days><option value="7">7 days</option><option value="30">30 days</option><option value="90">90 days</option></select></label>';
    html += '<p class="heat-key"><span class="heat-dot"></span> a click <span class="heat-ring"></span> a click on something that is not a link: they expected it to do something</p>';
    if (rows.length) { html += "<p><b>Clicked, but it goes nowhere</b></p><ol>" + rows.map(function (r) { return "<li></li>"; }).join("") + "</ol>"; }
    if (!clicks.length) html += "<p>No clicks recorded here yet. Visitors are counted from the day this went live; you are never counted while signed in.</p>";
    if (device && device !== (window.innerWidth < 600 ? "phone" : window.innerWidth < 1024 ? "tablet" : "desktop")) html += "<p>Tip: make this window about as wide as a " + esc(device) + " to see the page the way those visitors saw it.</p>";
    panel.innerHTML = html;
    var items = panel.querySelectorAll("ol li"); rows.forEach(function (r, i) { items[i].textContent = r[0] + " (" + r[1] + ")"; }); // visitor-page text is set as text, never as HTML
    var ds = panel.querySelector("[data-dev]"), dd = panel.querySelector("[data-days]"); ds.value = device; dd.value = String(days);
    ds.addEventListener("change", function () { device = ds.value; load(); });
    dd.addEventListener("change", function () { days = parseInt(dd.value, 10) || 30; load(); });
  }

  function load() {
    fetch("/admin/api/heatmap/?path=" + encodeURIComponent(location.pathname) + "&device=" + device + "&days=" + days, { credentials: "same-origin", headers: { "Accept": "application/json" } })
      .then(function (r) { return r.json(); })
      .then(function (j) { data = j; draw(); summarise(); })
      .catch(function () { panel.textContent = "The heat map could not be loaded. Try again."; });
  }

  var resizeTimer = null;
  window.addEventListener("resize", function () { if (!on || !data) return; clearTimeout(resizeTimer); resizeTimer = setTimeout(draw, 200); });
  btn.addEventListener("click", function () {
    on = !on; btn.setAttribute("aria-pressed", on ? "true" : "false"); btn.textContent = on ? "Hide heat map" : "Heat map";
    if (!on) { if (layer) layer.remove(); if (panel) panel.remove(); layer = panel = null; return; }
    panel = document.createElement("aside"); panel.className = "heat-panel"; panel.textContent = "Loading…"; document.body.appendChild(panel);
    load();
  });
})();
