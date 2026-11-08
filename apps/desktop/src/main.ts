import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

async function start() {
  // Dev server opened in an ordinary browser: stand in for the backend (see lib/preview/mock.ts).
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installPreviewMock } = await import("./lib/preview/mock");
    installPreviewMock();
  }
  return mount(App, { target: document.getElementById("app")! });
}

export default start();
