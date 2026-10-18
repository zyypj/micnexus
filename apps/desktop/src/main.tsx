import { cssVariables } from "@micnexus/ui";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

for (const [k, v] of Object.entries(cssVariables())) document.documentElement.style.setProperty(k, v);

// A desktop app, not a web page: no browser context menu outside text fields.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea")) e.preventDefault();
});

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
