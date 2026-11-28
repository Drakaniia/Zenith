// Entry of the PDF viewer page (pdf.html, served at zenith://pdf/ inside a tab's own webview).
// It is an ordinary untrusted page: no IPC bridge, only its document and save endpoints.
import { mount } from "svelte";
import PdfApp from "./PdfApp.svelte";

mount(PdfApp, { target: document.getElementById("app")! });
