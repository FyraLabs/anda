import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "@fontsource/inter/variable.css";
import "./main.scss";
import "xterm/css/xterm.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
