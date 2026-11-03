import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import App from "./App";
import { ProviderCatalogProvider } from "./ProviderCatalog";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ProviderCatalogProvider><App /></ProviderCatalogProvider>
  </React.StrictMode>
);
