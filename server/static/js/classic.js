/* Classic view: larger text, no 3D pen, no motion. Chosen with the footer switch and remembered in
   this browser. Runs in <head> so the page never flashes the other way first. */
(function () {
  "use strict";
  var on = false;
  try { on = localStorage.getItem("pm-classic") === "1"; } catch (e) { }
  if (on) document.documentElement.classList.add("classic");
  document.addEventListener("DOMContentLoaded", function () {
    var b = document.getElementById("classicToggle"); if (!b) return;
    var paint = function () { b.setAttribute("aria-pressed", on ? "true" : "false"); b.textContent = on ? "Classic view is on. Switch back to the standard view" : "Classic view: larger text, no animation"; };
    paint();
    b.addEventListener("click", function () {
      on = !on;
      try { localStorage.setItem("pm-classic", on ? "1" : "0"); } catch (e) { }
      location.reload();
    });
  });
})();
