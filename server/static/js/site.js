/* ThePenMarket.com page behaviour: menu sheet, footer CSRF, sort autosubmit, facet see-all,
   grid/list view, product gallery, filter rail on phones. No framework. */
(function () {
  "use strict";
  var $ = function (s, r) { return (r || document).querySelector(s); };
  var $$ = function (s, r) { return Array.prototype.slice.call((r || document).querySelectorAll(s)); };

  // menu sheet
  var menuBtn = $(".menu-btn"), sheet = $("#sheet");
  if (menuBtn && sheet) {
    menuBtn.addEventListener("click", function () {
      var open = sheet.hasAttribute("hidden");
      if (open) sheet.removeAttribute("hidden"); else sheet.setAttribute("hidden", "");
      menuBtn.setAttribute("aria-expanded", open ? "true" : "false");
      menuBtn.textContent = open ? "Close" : "Menu";
    });
  }

  // footer mailing-list form: copy the CSRF cookie into the hidden field (double-submit)
  var m = document.cookie.match(/(?:^|;\s*)pm_csrf=([0-9a-f]{32,})/);
  if (m) $$("form[data-csrf-from-cookie] input[name=csrf]").forEach(function (i) { if (!i.value) i.value = m[1]; });

  // sort select submits its form
  $$("select[data-autosubmit]").forEach(function (sel) { sel.addEventListener("change", function () { sel.form.submit(); }); });

  // facet "see all"
  $$("button[data-more]").forEach(function (b) {
    b.addEventListener("click", function () {
      var list = b.previousElementSibling; list.classList.toggle("is-open");
      b.textContent = list.classList.contains("is-open") ? "See fewer" : "See all " + list.children.length;
    });
  });

  // grid / list view, remembered
  var grid = $("#resultsGrid");
  if (grid) {
    var apply = function (v) {
      grid.classList.toggle("is-list", v === "list");
      $$(".view-btn").forEach(function (x) { var on = x.getAttribute("data-view") === v; x.classList.toggle("is-on", on); x.setAttribute("aria-pressed", on ? "true" : "false"); });
    };
    try { apply(localStorage.getItem("pm-view") || "grid"); } catch (e) { apply("grid"); }
    $$(".view-btn").forEach(function (x) { x.addEventListener("click", function () { var v = x.getAttribute("data-view"); apply(v); try { localStorage.setItem("pm-view", v); } catch (e) { } }); });
  }

  // filter rail toggle on phones
  var railToggle = $(".rail-toggle"), rail = $("#rail");
  if (railToggle && rail) railToggle.addEventListener("click", function () { var open = rail.classList.toggle("is-open"); railToggle.setAttribute("aria-expanded", open ? "true" : "false"); });

  // product gallery
  var img = $("#galleryImg"), link = $("#galleryLink");
  if (img) $$(".thumb").forEach(function (t) {
    t.addEventListener("click", function () {
      img.src = t.getAttribute("data-large"); img.alt = t.getAttribute("data-alt") || "";
      if (link) link.href = t.getAttribute("data-full");
      $$(".thumb").forEach(function (x) { x.classList.toggle("is-on", x === t); });
    });
  });

  // glossary index: filter as you type and by kind, without a round trip (Enter still asks the server,
  // which is how a word with no entry gets noted for Nathaniel)
  var glossInput = $("[data-gloss-filter]");
  if (glossInput) {
    var glossRows = $$("[data-gloss-row]"), glossGroups = $$("[data-gloss-group]"), glossStatus = $("[data-gloss-status]"), glossNone = $("[data-gloss-none]"), glossKinds = $$("[data-gloss-kind]");
    var glossKind = (glossKinds.filter(function (k) { return k.getAttribute("aria-current") === "true"; })[0] || { getAttribute: function () { return ""; } }).getAttribute("data-gloss-kind") || "";
    var fold = function (v) { return v.toLowerCase().replace(/[’']/g, "").replace(/[^a-z0-9\u00c0-\u024f]+/g, " ").trim(); };
    var glossFilter = function () {
      var q = fold(glossInput.value), shown = 0;
      glossRows.forEach(function (r) {
        var ok = (!glossKind || r.getAttribute("data-kind") === glossKind) && (!q || r.getAttribute("data-search").indexOf(q) !== -1);
        r.hidden = !ok; if (ok) shown++;
      });
      glossGroups.forEach(function (g) { g.hidden = !g.querySelector("[data-gloss-row]:not([hidden])"); });
      $$("[data-gloss-letter]").forEach(function (l) { var g = document.getElementById("letter-" + l.getAttribute("data-gloss-letter")); l.classList.toggle("is-off", !g || g.hidden); });
      if (glossNone) glossNone.hidden = shown > 0 || !glossRows.length;
      if (glossStatus && (q || glossKind)) glossStatus.textContent = shown + (shown === 1 ? " entry" : " entries") + (q ? " for “" + glossInput.value.trim() + "”" : "") + ".";
    };
    glossInput.addEventListener("input", glossFilter);
    glossKinds.forEach(function (k) {
      k.addEventListener("click", function (e) {
        e.preventDefault(); glossKind = k.getAttribute("data-gloss-kind") || "";
        glossKinds.forEach(function (x) { if (x === k) x.setAttribute("aria-current", "true"); else x.removeAttribute("aria-current"); });
        glossFilter();
      });
    });
  }

  // tap-to-define: a glossary word in a listing opens a small card with his definition; the link
  // itself still goes to the entry (second tap, the card's link, or any click without JavaScript)
  var glPop = null, glFor = null;
  var glClose = function () { if (glPop) { glPop.remove(); glPop = null; } if (glFor) { glFor.removeAttribute("aria-describedby"); glFor = null; } };
  document.addEventListener("click", function (e) {
    var a = e.target.closest ? e.target.closest("a.gl") : null;
    if (!a) { if (glPop && !glPop.contains(e.target)) glClose(); return; }
    if (e.metaKey || e.ctrlKey || e.shiftKey || glFor === a) return;
    e.preventDefault(); glClose();
    glPop = document.createElement("div"); glPop.className = "gl-pop"; glPop.id = "glPop"; glPop.setAttribute("role", "tooltip");
    var h = document.createElement("b"); h.textContent = a.getAttribute("data-gl-name") || a.textContent;
    var p = document.createElement("p"); p.textContent = a.getAttribute("data-gl") || "";
    var more = document.createElement("a"); more.href = a.getAttribute("href"); more.textContent = "Read the glossary entry";
    glPop.appendChild(h); glPop.appendChild(p); glPop.appendChild(more);
    document.body.appendChild(glPop);
    var r = a.getBoundingClientRect(), w = glPop.offsetWidth, left = Math.max(12, Math.min(r.left + window.scrollX, window.scrollX + document.documentElement.clientWidth - w - 12));
    glPop.style.left = left + "px"; glPop.style.top = (r.bottom + window.scrollY + 8) + "px";
    glFor = a; a.setAttribute("aria-describedby", "glPop");
  });
  document.addEventListener("keydown", function (e) { if (e.key === "Escape") glClose(); });

  // "Ask the guide about this pen" links land on the home page with a sku hint the guide reads
  if (location.hash.indexOf("#ask") === 0) { var a = $("#ask"); if (a) setTimeout(function () { a.scrollIntoView({ block: "start" }); }, 50); }
})();
