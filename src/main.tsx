import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import App from "./App";

const overlayKind = new URLSearchParams(window.location.search).get("overlay");
if (overlayKind) {
  document.documentElement.dataset.overlay = overlayKind;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

function dismissSplash() {
  const el = document.getElementById("splash");
  if (!el) return;
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduce) {
    el.remove();
    return;
  }
  el.classList.add("is-leaving");
  window.setTimeout(() => el.remove(), 360);
}

// Fade the boot splash after the first commit; it covered the WebView while
// the JS bundle loaded (noticeable in dev mode).
requestAnimationFrame(() => {
  requestAnimationFrame(dismissSplash);
});
