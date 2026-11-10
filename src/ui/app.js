// Small glue around htmx. No inline script is allowed by the CSP, so this is
// served from the UI origin.
(function () {
  "use strict";

  function isPoll(e) {
    var d = e.detail || {};
    var el = (d.requestConfig && d.requestConfig.elt) || d.elt;
    return el && el.hasAttribute && el.hasAttribute("data-poll") ? el : null;
  }

  // Polling pauses while the tab is hidden: the timer keeps ticking but the
  // request is cancelled, and the first tick after the tab is shown refreshes.
  document.addEventListener("htmx:beforeRequest", function (e) {
    if (isPoll(e) && document.hidden) e.preventDefault();
  });

  // A refresh must not wipe what the Reviewer is typing: skip the swap while a
  // compose form is open or any textarea holds text. The next poll retries.
  document.addEventListener("htmx:beforeSwap", function (e) {
    var el = isPoll(e);
    if (!el) return;
    if (el.querySelector("form.compose")) {
      e.detail.shouldSwap = false;
      return;
    }
    var boxes = el.querySelectorAll("textarea");
    for (var i = 0; i < boxes.length; i++) {
      if (boxes[i].value.trim() !== "" || boxes[i] === document.activeElement) {
        e.detail.shouldSwap = false;
        return;
      }
    }
  });

  document.addEventListener("click", function (e) {
    var b = e.target.closest && e.target.closest("[data-cancel]");
    var f = b && b.closest("form");
    if (f) f.remove();
  });

  document.addEventListener("htmx:responseError", function (e) {
    window.alert(e.detail.xhr.responseText || "request failed");
  });
})();
