import React from "react";
import { createRoot } from "react-dom/client";
import "./styles/tokens.css";
import "./styles/ui.css";
import { SessionProvider, WindowsProvider, ToastProvider } from "./lib/store";
import { App } from "./App";

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SessionProvider>
      <WindowsProvider>
        <ToastProvider>
          <App />
        </ToastProvider>
      </WindowsProvider>
    </SessionProvider>
  </React.StrictMode>,
);
