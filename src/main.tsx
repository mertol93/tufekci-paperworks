import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { I18nProvider } from "./I18nProvider";
import "pdfjs-dist/web/pdf_viewer.css";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <I18nProvider>
      <App />
    </I18nProvider>
  </React.StrictMode>
);
