// Core
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
// Components
import App from "@/App/App";
import I18nProvider from "@/i18n/I18nProvider";
// Utils
import markPlatform from "@/utils/platform";

markPlatform();

// A desktop app has no use for the webview's "Reload / Inspect" menu outside development.
if (import.meta.env.PROD) {
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

const root = document.getElementById("root");
if (!root) throw new Error("index.html must contain #root");

createRoot(root).render(
  <StrictMode>
    <I18nProvider>
      <App />
    </I18nProvider>
  </StrictMode>,
);
