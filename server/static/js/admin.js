/* Click-to-edit for the signed-in admin (SPEC-0001). Only loaded with a session. Every save is a
   JSON request with the CSRF token; a 428 means "re-enter your password", handled here. */
(function () {
  "use strict";
  var bar = document.getElementById("admBar");
  if (!bar) return; // preview mode: no editing
  var csrf = bar.getAttribute("data-csrf") || "";
  document.documentElement.classList.add("editing");
  var $ = function (s, r) { return (r || document).querySelector(s); };
  var $$ = function (s, r) { return Array.prototype.slice.call((r || document).querySelectorAll(s)); };

  function toast(msg, err) {
    var t = document.createElement("div"); t.className = "adm-toast" + (err ? " err" : ""); t.textContent = msg;
    document.body.appendChild(t); setTimeout(function () { t.remove(); }, err ? 5000 : 2600);
  }
  function api(method, url, body, isForm) {
    var opts = { method: method, headers: { "X-CSRF": csrf, "Accept": "application/json" }, credentials: "same-origin" };
    if (body && !isForm) { opts.headers["Content-Type"] = "application/json"; opts.body = JSON.stringify(body); }
    if (body && isForm) opts.body = body;
    return fetch(url, opts).then(function (r) { return r.json().then(function (j) { j._status = r.status; return j; }); });
  }
  function stepUp() {
    return new Promise(function (resolve) {
      var m = document.createElement("div"); m.className = "adm-modal";
      m.innerHTML = '<form><h2>Please confirm it\'s you</h2><p>This change is one we double-check. Enter your password to continue.</p><input type="password" name="pw" autocomplete="current-password" required autofocus><div class="row"><button type="button" class="adm-editor cancel" data-x>Cancel</button><button type="submit" class="adm-btn adm-red">Confirm</button></div></form>';
      document.body.appendChild(m);
      var f = m.querySelector("form");
      f.querySelector("[data-x]").addEventListener("click", function () { m.remove(); resolve(false); });
      f.addEventListener("submit", function (e) {
        e.preventDefault();
        api("POST", "/admin/step-up/", { password: f.pw.value }).then(function (j) { if (j.ok) { m.remove(); resolve(true); } else { toast(j.error || "That password is wrong.", true); } });
      });
      f.pw.focus();
    });
  }
  function save(field, value, then) {
    var pid = ($("[data-product-id]") || {}).getAttribute ? $("[data-product-id]").getAttribute("data-product-id") : null;
    if (!pid) return;
    api("PATCH", "/admin/api/product/" + pid + "/", { field: field, value: value }).then(function (j) {
      if (j._status === 428) { return stepUp().then(function (ok) { if (ok) save(field, value, then); }); }
      if (!j.ok) { toast(j.error || "That didn't save.", true); return; }
      toast("Saved."); then(j);
    }).catch(function () { toast("The connection dropped. Try again.", true); });
  }

  // ---- inline editors --------------------------------------------------------------------------
  var options = null;
  function loadOptions() { return options ? Promise.resolve(options) : fetch("/admin/api/options/", { credentials: "same-origin" }).then(function (r) { return r.json(); }).then(function (o) { options = o; return o; }); }
  var pickers = { brand: "brand", era: "era", nib: "nib", filling_mechanism: "filling_mechanism", category: "category" };

  function openEditor(el) {
    if (el.classList.contains("is-editing")) return;
    var field = el.getAttribute("data-edit");
    var current = el.getAttribute("data-slug") !== null && pickers[field] ? el.getAttribute("data-slug") : (field === "description" || field === "tags" ? (el.getAttribute("data-text") || "") : el.textContent.trim());
    var original = el.innerHTML;
    el.classList.add("is-editing");
    var box = document.createElement("span"); box.className = "adm-editor";
    var input;
    function close(restore) { el.classList.remove("is-editing"); if (restore) el.innerHTML = original; }
    function done(j) {
      if (field === "description") { el.innerHTML = j.html || ""; el.setAttribute("data-text", input.value); }
      else if (field === "tags") { el.innerHTML = j.html || ""; el.setAttribute("data-text", j.display || ""); }
      else if (pickers[field]) { var sel = input; var opt = sel.options[sel.selectedIndex]; el.setAttribute("data-slug", sel.value); el.textContent = sel.value ? opt.textContent : ""; }
      else { el.textContent = j.display; }
      if (field === "status") { document.body.setAttribute("data-status", j.status); }
      el.classList.remove("is-editing"); el.classList.toggle("is-empty", !el.textContent.trim());
    }
    if (field === "description") {
      input = document.createElement("textarea"); input.value = current;
      box.appendChild(input);
      var h = document.createElement("span"); h.className = "hint"; h.textContent = "Plain text. Leave a blank line between paragraphs. Written exactly as you would for a listing."; box.appendChild(h);
    } else if (pickers[field]) {
      input = document.createElement("select");
      var none = document.createElement("option"); none.value = ""; none.textContent = field === "category" ? "Choose…" : "None"; input.appendChild(none);
      loadOptions().then(function (o) { (o[pickers[field]] || []).forEach(function (t) { var op = document.createElement("option"); op.value = t.slug; op.textContent = t.name; if (t.slug === current) op.selected = true; input.appendChild(op); }); });
      box.appendChild(input);
    } else {
      input = document.createElement("input"); input.type = "text"; input.value = current; input.setAttribute("aria-label", field);
      if (field === "price" || field === "sale_price") { input.inputMode = "decimal"; input.placeholder = field === "sale_price" ? "blank = no sale" : "149.99"; }
      if (field === "length_cm") input.placeholder = "13.4";
      box.appendChild(input);
      if (field === "tags") {
        input.placeholder = "Gift, Desk set, Parker 51";
        var th = document.createElement("span"); th.className = "hint"; th.textContent = "Separate tags with commas. Each tag gets its own page listing everything with it."; box.appendChild(th);
        // His earlier tags, offered as he types, so "Gift" and "gifts" do not become two tags.
        var names = [];
        var fill = function () {
          var dl = document.getElementById("adm-tag-list"); if (!dl) return;
          var head = input.value.replace(/[^,]*$/, ""), prefix = head ? head.replace(/\s*$/, " ") : "";
          dl.innerHTML = "";
          names.forEach(function (t) { var op = document.createElement("option"); op.value = prefix + t; dl.appendChild(op); });
        };
        loadOptions().then(function (o) {
          names = o.tags || [];
          var dl = document.createElement("datalist"); dl.id = "adm-tag-list"; box.appendChild(dl);
          input.setAttribute("list", "adm-tag-list"); fill();
        });
        input.addEventListener("input", fill);
      }
    }
    var ok = document.createElement("button"); ok.type = "button"; ok.className = "save"; ok.textContent = "Save";
    var no = document.createElement("button"); no.type = "button"; no.className = "cancel"; no.textContent = "Cancel";
    box.appendChild(ok); box.appendChild(no);
    el.innerHTML = ""; el.appendChild(box); input.focus();
    ok.addEventListener("click", function () { save(field, input.value, function (j) { done(j); }); });
    no.addEventListener("click", function () { close(true); });
    input.addEventListener("keydown", function (e) { if (e.key === "Escape") close(true); if (e.key === "Enter" && field !== "description") { e.preventDefault(); ok.click(); } });
  }
  $$("[data-edit]").forEach(function (el) {
    if (!el.textContent.trim()) el.classList.add("is-empty");
    el.addEventListener("click", function (e) {
      if (el.classList.contains("is-editing")) return;
      e.preventDefault(); e.stopPropagation(); openEditor(el);
    });
  });

  // ---- status control on the product page -------------------------------------------------------
  var art = $("[data-product-id]");
  if (art) {
    var st = art.getAttribute("data-product-status") || "live";
    var wrap = document.createElement("div"); wrap.className = "adm-status";
    wrap.innerHTML = '<label>Status <select><option value="live">Live (customers can see it)</option><option value="draft">Hidden</option><option value="sold">Sold</option></select></label>';
    var sel = wrap.querySelector("select"); sel.value = st === "archived" ? "draft" : st;
    var buy = $(".buybox .eyebrow"); if (buy) buy.insertAdjacentElement("afterend", wrap);
    sel.addEventListener("change", function () { var v = sel.value; save("status", v, function () { location.reload(); }); });
  }

  // ---- photos: reorder by drag, add, remove -------------------------------------------------------
  var thumbs = $("#thumbs");
  if (art && !thumbs) { thumbs = document.createElement("div"); thumbs.className = "thumbs"; thumbs.id = "thumbs"; var g = $("#gallery"); if (g) g.appendChild(thumbs); }
  if (art && thumbs) {
    thumbs.classList.add("adm-photos");
    var pid = art.getAttribute("data-product-id");
    function decorate(t) {
      if (t.querySelector(".rm")) return;
      t.setAttribute("draggable", "true");
      var rm = document.createElement("button"); rm.type = "button"; rm.className = "rm"; rm.textContent = "×"; rm.title = "Remove this photo";
      rm.addEventListener("click", function (e) {
        e.stopPropagation();
        var id = t.getAttribute("data-image-id");
        api("POST", "/admin/api/photo/" + id + "/archive/", {}).then(function (j) {
          if (j._status === 428) return stepUp().then(function (ok) { if (ok) rm.click(); });
          if (!j.ok) { toast(j.error || "Couldn't remove it.", true); return; }
          t.remove(); toast("Photo removed. It's kept in History and can be put back.");
        });
      });
      t.appendChild(rm);
      t.addEventListener("dragstart", function () { t.classList.add("dragging"); });
      t.addEventListener("dragend", function () { t.classList.remove("dragging"); sendOrder(); });
    }
    function sendOrder() {
      var ids = $$(".thumb", thumbs).map(function (x) { return parseInt(x.getAttribute("data-image-id"), 10); }).filter(function (n) { return !isNaN(n); });
      api("POST", "/admin/api/product/" + pid + "/photos/order/", { ids: ids }).then(function (j) { if (!j.ok) toast(j.error || "Order not saved.", true); else toast("Photo order saved."); });
    }
    thumbs.addEventListener("dragover", function (e) {
      e.preventDefault();
      var dragging = $(".thumb.dragging", thumbs); if (!dragging) return;
      var after = $$(".thumb:not(.dragging)", thumbs).find(function (x) { var r = x.getBoundingClientRect(); return e.clientX < r.left + r.width / 2 && e.clientY < r.bottom; });
      if (after) thumbs.insertBefore(dragging, after); else thumbs.insertBefore(dragging, add);
    });
    $$(".thumb", thumbs).forEach(decorate);
    var add = document.createElement("label"); add.className = "adm-add-photo"; add.innerHTML = '+ Photo<input type="file" accept="image/*" capture="environment">';
    thumbs.appendChild(add);
    var maxPhotos = 14;
    function photoCount() { return $$(".thumb", thumbs).length; }
    function addThumb(j) {
      var b = document.createElement("button"); b.type = "button"; b.className = "thumb"; b.setAttribute("data-image-id", j.id); b.setAttribute("data-large", j.large); b.setAttribute("data-full", j.full);
      b.innerHTML = '<img src="' + j.thumb + '" alt="">'; thumbs.insertBefore(b, add); decorate(b);
      var big = $("#galleryImg"); if (big && !big.getAttribute("src")) big.src = j.large;
    }

    // ---- guided photo shoot: the site asks for each shot on his list, names it and files it in order ----
    var guided = document.createElement("button"); guided.type = "button"; guided.className = "adm-guided"; guided.textContent = "Guided photo shoot";
    thumbs.appendChild(guided);
    guided.addEventListener("click", function () {
      loadOptions().then(function (o) {
        maxPhotos = o.max_photos || maxPhotos;
        var shots = (o.shots || []).slice(), i = 0, taken = 0;
        var m = document.createElement("div"); m.className = "adm-modal";
        m.innerHTML = '<form class="adm-shoot"><p class="adm-shoot-step"></p><h2></h2><p class="adm-shoot-hint">Take the photo with this phone or tablet, or choose one you already have. It is named and put in the right place for you.</p><label class="adm-btn adm-red adm-shoot-pick">Take or choose the photo<input type="file" accept="image/*" capture="environment" hidden></label><div class="row"><button type="button" class="adm-editor cancel" data-skip>Skip this shot</button><button type="button" class="adm-editor cancel" data-done>Finish</button></div></form>';
        document.body.appendChild(m);
        var step = m.querySelector(".adm-shoot-step"), head = m.querySelector("h2"), pick = m.querySelector("input[type=file]");
        function close() { m.remove(); if (taken) toast(taken + (taken === 1 ? " photo" : " photos") + " added, in shot order."); }
        function show() {
          if (i >= shots.length || photoCount() >= maxPhotos) { close(); if (photoCount() >= maxPhotos) toast("That's " + maxPhotos + " photos, the most a pen holds."); return; }
          step.textContent = "Shot " + (i + 1) + " of " + shots.length; head.textContent = shots[i];
        }
        m.querySelector("[data-skip]").addEventListener("click", function () { i++; show(); });
        m.querySelector("[data-done]").addEventListener("click", close);
        pick.addEventListener("change", function () {
          var f = pick.files && pick.files[0]; pick.value = ""; if (!f) return;
          if (f.size > 10 * 1024 * 1024) { toast("Photos must be under 10 MB.", true); return; }
          var fd = new FormData(); fd.append("shot", shots[i]); fd.append("photo", f);
          head.textContent = "Uploading…";
          api("POST", "/admin/api/product/" + pid + "/photo/", fd, true).then(function (j) {
            if (!j.ok) { toast(j.error || "Upload failed.", true); show(); return; }
            addThumb(j); taken++; i++; show();
          }).catch(function () { toast("The connection dropped. Try that shot again.", true); show(); });
        });
        show();
      });
    });
    add.querySelector("input").addEventListener("change", function () {
      var f = this.files && this.files[0]; if (!f) return;
      if (f.size > 10 * 1024 * 1024) { toast("Photos must be under 10 MB.", true); return; }
      var fd = new FormData(); fd.append("photo", f);
      toast("Uploading…");
      api("POST", "/admin/api/product/" + pid + "/photo/", fd, true).then(function (j) {
        if (!j.ok) { toast(j.error || "Upload failed.", true); return; }
        addThumb(j); toast("Photo added.");
      });
      this.value = "";
    });
  }

  // ---- feature this pen on the homepage -----------------------------------------------------------
  if (art && (art.getAttribute("data-product-status") || "live") === "live") {
    var featPid = parseInt(art.getAttribute("data-product-id"), 10);
    var feat = document.createElement("button"); feat.type = "button"; feat.className = "adm-feature";
    var statusBox = $(".adm-status"); if (statusBox) statusBox.insertAdjacentElement("afterend", feat);
    var isFeatured = false;
    var paintFeat = function () { feat.textContent = isFeatured ? "★ On the homepage now. Click to go back to the newest pen" : "☆ Feature on the homepage"; feat.setAttribute("aria-pressed", isFeatured ? "true" : "false"); };
    loadOptions().then(function (o) { isFeatured = o.featured_product_id === featPid; paintFeat(); });
    feat.addEventListener("click", function () {
      api("POST", "/admin/api/featured/", { product_id: isFeatured ? null : featPid }).then(function (j) {
        if (!j.ok) { toast(j.error || "That didn't save.", true); return; }
        isFeatured = !isFeatured; options = null; paintFeat();
        toast(isFeatured ? "This pen now leads the homepage." : "The homepage is back to the newest pen.");
      });
    });
  }

  // ---- add a pen --------------------------------------------------------------------------------
  var addPen = $("#admAddPen");
  if (addPen) addPen.addEventListener("click", function () {
    var name = window.prompt("What should the new pen be called? (You can change it after.)", "");
    if (name === null) return;
    api("POST", "/admin/api/product/", { title: name }).then(function (j) { if (j.ok) location.href = j.url; else toast(j.error || "Couldn't add it.", true); });
  });
})();
