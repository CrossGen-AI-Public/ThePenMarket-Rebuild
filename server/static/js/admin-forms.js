/* Show-password toggle on the sign-in and reset forms. */
(function () {
  var cb = document.getElementById("showpw"); if (!cb) return;
  cb.addEventListener("change", function () {
    Array.prototype.forEach.call(document.querySelectorAll('input[type="password"], input[data-pw]'), function (i) { i.setAttribute("data-pw", "1"); i.type = cb.checked ? "text" : "password"; });
  });
})();
