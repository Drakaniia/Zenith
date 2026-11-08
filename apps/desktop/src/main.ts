import { mount } from "svelte";
import "./styles/app.css";
import "./app/actions";
import App from "./app/App.svelte";

async function start() {
  // Dev server opened in an ordinary browser: stand in for the backend (see app/preview/mock.ts).
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installPreviewMock } = await import("./app/preview/mock");
    installPreviewMock();
  }
  return mount(App, { target: document.getElementById("app")! });
}

export default start();
