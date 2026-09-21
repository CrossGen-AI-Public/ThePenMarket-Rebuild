/* The writing box on the admin's post and page forms: a small toolbar over an editable area, photos
   uploaded to this server, and the result copied into the form's textarea when it is saved. The
   server cleans whatever arrives (admin/content.rs clean_html), so this file is convenience only.
   Without JavaScript the plain textarea still works. */
(function () {
  "use strict";
  var csrf = (document.getElementById("admBar") || { getAttribute: function () { return ""; } }).getAttribute("data-csrf") || "";
  function toast(msg, err) {
    var t = document.createElement("div"); t.className = "adm-toast" + (err ? " err" : ""); t.textContent = msg;
    document.body.appendChild(t); setTimeout(function () { t.remove(); }, err ? 5000 : 2600);
  }
  function upload(file) {
    if (file.size > 10 * 1024 * 1024) { toast("Photos must be under 10 MB.", true); return Promise.resolve(null); }
    var fd = new FormData(); fd.append("photo", file);
    toast("Uploading…");
    return fetch("/admin/api/media/", { method: "POST", body: fd, headers: { "X-CSRF": csrf, "Accept": "application/json" }, credentials: "same-origin" })
      .then(function (r) { return r.json(); })
      .then(function (j) { if (!j.ok) { toast(j.error || "Upload failed.", true); return null; } return j; })
      .catch(function () { toast("The connection dropped. Try again.", true); return null; });
  }
  function esc(s) { return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;"); }

  Array.prototype.forEach.call(document.querySelectorAll("[data-editor]"), function (box) {
    var area = box.querySelector("textarea"); if (!area) return;
    var bar = document.createElement("div"); bar.className = "adm-rte-bar"; bar.setAttribute("role", "toolbar"); bar.setAttribute("aria-label", "Formatting");
    var page = document.createElement("div"); page.className = "adm-rte-page prose"; page.contentEditable = "true"; page.setAttribute("role", "textbox"); page.setAttribute("aria-multiline", "true"); page.setAttribute("aria-label", "The text");
    page.innerHTML = area.value;
    var file = document.createElement("input"); file.type = "file"; file.accept = "image/*"; file.hidden = true;
    var source = false;

    function cmd(name, arg) { page.focus(); document.execCommand(name, false, arg || null); }
    var buttons = [
      ["Bold", function () { cmd("bold"); }], ["Italic", function () { cmd("italic"); }],
      ["Heading", function () { cmd("formatBlock", "h2"); }], ["Small heading", function () { cmd("formatBlock", "h3"); }], ["Normal text", function () { cmd("formatBlock", "p"); }],
      ["Quote", function () { cmd("formatBlock", "blockquote"); }], ["List", function () { cmd("insertUnorderedList"); }], ["Numbered list", function () { cmd("insertOrderedList"); }],
      ["Link", function () {
        var url = window.prompt("Where should the link go? Paste the address (for a page on this site, start with /).", "https://");
        if (url === null) return; url = url.trim();
        if (!url || url === "https://") { cmd("unlink"); return; }
        if (!/^(https?:\/\/|\/|mailto:|tel:)/i.test(url)) url = "https://" + url;
        cmd("createLink", url);
      }],
      ["Link to a blog post", function () {
        // Pick one of his posts by title; the selected words become the link (or the title is inserted).
        var sel = window.getSelection(), saved = sel.rangeCount ? sel.getRangeAt(0).cloneRange() : null;
        var m = document.createElement("div"); m.className = "adm-modal";
        m.innerHTML = '<form class="adm-postpick"><h2>Link to a blog post</h2><input type="search" placeholder="Type part of the title" aria-label="Search your posts"><ul></ul><div class="row"><button type="button" class="adm-editor cancel">Cancel</button></div></form>';
        document.body.appendChild(m);
        var input = m.querySelector("input"), list = m.querySelector("ul"), timer = null;
        function close() { m.remove(); }
        function search() {
          fetch("/admin/api/posts/?q=" + encodeURIComponent(input.value), { credentials: "same-origin", headers: { "Accept": "application/json" } }).then(function (r) { return r.json(); }).then(function (j) {
            list.innerHTML = "";
            (j.posts || []).forEach(function (p) {
              var li = document.createElement("li"), b = document.createElement("button"); b.type = "button"; b.textContent = p.title;
              b.addEventListener("click", function () {
                close(); page.focus();
                if (saved) { sel.removeAllRanges(); sel.addRange(saved); }
                if (saved && !saved.collapsed) cmd("createLink", p.url); else cmd("insertHTML", '<a href="' + esc(p.url) + '">' + esc(p.title) + "</a>");
              });
              li.appendChild(b); list.appendChild(li);
            });
            if (!list.children.length) { var none = document.createElement("li"); none.textContent = "No post has that in its title."; list.appendChild(none); }
          });
        }
        input.addEventListener("input", function () { clearTimeout(timer); timer = setTimeout(search, 200); });
        m.querySelector(".cancel").addEventListener("click", close);
        m.querySelector("form").addEventListener("submit", function (e) { e.preventDefault(); });
        input.focus(); search();
      }],
      ["Photo", function () { file.click(); }], ["Undo", function () { cmd("undo"); }],
      ["Show the HTML", function (b) {
        source = !source;
        if (source) { area.value = page.innerHTML; } else { page.innerHTML = area.value; }
        area.hidden = !source; page.hidden = source; b.textContent = source ? "Back to the editor" : "Show the HTML";
      }]
    ];
    buttons.forEach(function (def) {
      var b = document.createElement("button"); b.type = "button"; b.textContent = def[0];
      b.addEventListener("mousedown", function (e) { e.preventDefault(); }); // keep the selection in the text
      b.addEventListener("click", function () { def[1](b); });
      bar.appendChild(b);
    });
    file.addEventListener("change", function () {
      var f = file.files && file.files[0]; file.value = ""; if (!f) return;
      upload(f).then(function (j) {
        if (!j) return;
        var alt = window.prompt("Describe the photo in a few words (for people who cannot see it, and for Google).", "") || "";
        cmd("insertHTML", '<figure><img src="' + esc(j.url) + '" alt="' + esc(alt) + '" width="' + j.width + '" height="' + j.height + '" loading="lazy"></figure><p><br></p>');
        toast("Photo added.");
      });
    });
    // Click a photo to select it, so Delete removes it (this is how a broken photo is cleared out).
    page.addEventListener("click", function (e) {
      Array.prototype.forEach.call(page.querySelectorAll("img.is-picked"), function (i) { i.classList.remove("is-picked"); });
      if (e.target.tagName === "IMG") { e.target.classList.add("is-picked"); var r = document.createRange(); r.selectNode(e.target.closest("figure") || e.target); var s = window.getSelection(); s.removeAllRanges(); s.addRange(r); }
    });
    area.hidden = true;
    box.insertBefore(bar, area); box.insertBefore(page, area); box.appendChild(file);
    try { document.execCommand("defaultParagraphSeparator", false, "p"); } catch (e) { }
    var form = box.closest("form"), dirty = false;
    page.addEventListener("input", function () { dirty = true; });
    if (form) form.addEventListener("submit", function () {
      if (!source) { Array.prototype.forEach.call(page.querySelectorAll("img.is-picked"), function (i) { i.classList.remove("is-picked"); }); area.value = page.innerHTML; }
      dirty = false;
    });
    window.addEventListener("beforeunload", function (e) { if (dirty) { e.preventDefault(); e.returnValue = ""; } });
  });

  // The photo at the top of a post.
  Array.prototype.forEach.call(document.querySelectorAll("[data-featured]"), function (box) {
    var img = box.querySelector("[data-featured-img]"), path = box.querySelector("input[name=featured_path]"), input = box.querySelector("[data-featured-file]"), clear = box.querySelector("[data-featured-clear]");
    input.addEventListener("change", function () {
      var f = input.files && input.files[0]; input.value = ""; if (!f) return;
      upload(f).then(function (j) { if (!j) return; path.value = j.path; img.src = j.url; img.hidden = false; clear.hidden = false; toast("Photo ready. Save the post to keep it."); });
    });
    clear.addEventListener("click", function () { path.value = ""; img.hidden = true; clear.hidden = true; });
  });
})();
