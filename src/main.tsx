import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { createAppearance } from "./appearance";

const appearance = createAppearance(() => window.localStorage, window.matchMedia("(prefers-color-scheme: dark)"), document.documentElement);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App appearance={appearance} />
  </React.StrictMode>,
);
