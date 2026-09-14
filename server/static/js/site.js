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

  // "Ask the guide about this pen" links land on the home page with a sku hint the guide reads
  if (location.hash.indexOf("#ask") === 0) { var a = $("#ask"); if (a) setTimeout(function () { a.scrollIntoView({ block: "start" }); }, 50); }
})();
