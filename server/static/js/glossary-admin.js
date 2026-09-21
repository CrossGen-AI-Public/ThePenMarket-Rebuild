/* The glossary entry editor: "Use these words" copies one of his own sentences into the definition box
   (and notes where it came from), the photo picker uploads to this server, and Delete asks first.
   Without JavaScript the form still saves; only these conveniences are missing. */
(function () {
  "use strict";
  var csrf = (document.getElementById("admBar") || { getAttribute: function () { return ""; } }).getAttribute("data-csrf") || "";
  function toast(msg, err) {
    var t = document.createElement("div"); t.className = "adm-toast" + (err ? " err" : ""); t.textContent = msg;
    document.body.appendChild(t); setTimeout(function () { t.remove(); }, err ? 5000 : 2600);
  }

  var form = document.querySelector("[data-gloss-form]");
  if (form) {
    var def = form.querySelector('textarea[name="definition"]');
    var srcUrl = form.querySelector('input[name="source_url"]'), srcTitle = form.querySelector('input[name="source_title"]');
    Array.prototype.forEach.call(form.querySelectorAll("[data-use-quote]"), function (btn) {
      btn.addEventListener("click", function () {
        var quote = btn.closest("blockquote").querySelector("[data-quote]").textContent.trim();
        def.value = def.value.trim() ? def.value.replace(/\s+$/, "") + "\n\n" + quote : quote;
        if (!srcUrl.value.trim()) { srcUrl.value = btn.getAttribute("data-source-url"); srcTitle.value = btn.getAttribute("data-source-title"); }
        def.focus();
      });
    });

    var box = form.querySelector("[data-gloss-photo]");
    if (box) {
      var path = box.querySelector('input[name="image_path"]'), img = box.querySelector("img"), file = box.querySelector('input[type="file"]'), clear = box.querySelector("[data-photo-clear]");
      file.addEventListener("change", function () {
        var f = file.files && file.files[0]; if (!f) return;
        if (f.size > 10 * 1024 * 1024) { toast("Photos must be under 10 MB.", true); return; }
        var fd = new FormData(); fd.append("photo", f);
        toast("Uploading…");
        fetch("/admin/api/media/", { method: "POST", body: fd, headers: { "X-CSRF": csrf, "Accept": "application/json" }, credentials: "same-origin" })
          .then(function (r) { return r.json(); })
          .then(function (j) {
            if (!j.ok) { toast(j.error || "Upload failed.", true); return; }
            path.value = j.url; img.src = j.url; img.hidden = false; clear.hidden = false;
            toast("Photo added. Save the entry to keep it.");
          })
          .catch(function () { toast("The connection dropped. Try again.", true); });
      });
      clear.addEventListener("click", function () { path.value = ""; img.hidden = true; clear.hidden = true; file.value = ""; });
    }
  }

  Array.prototype.forEach.call(document.querySelectorAll("form[data-confirm]"), function (f) {
    f.addEventListener("submit", function (e) { if (!window.confirm(f.getAttribute("data-confirm"))) e.preventDefault(); });
  });
})();
