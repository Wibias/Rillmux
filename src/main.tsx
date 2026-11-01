import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import App from "./App";

const overlayKind = new URLSearchParams(window.location.search).get("overlay");
if (overlayKind) {
  document.documentElement.dataset.overlay = overlayKind;
}

function BootSplashGate() {
  // Dismiss from a commit effect, not requestAnimationFrame: the native splash
  // keeps the main window hidden, and a hidden document does not run rAF.
  useEffect(() => {
    dismissSplash();
  }, []);
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BootSplashGate />
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
