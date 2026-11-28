// The pdf.js viewer components behind the PDF page: rendering, text selection, links, form
// fields and pdf.js's own annotation editors (text, ink, highlight). Saving writes the form
// values and annotations into the file.

import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentProxy } from "pdfjs-dist";
import workerSrc from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { EventBus, LinkTarget, PDFFindController, PDFLinkService, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
import "pdfjs-dist/web/pdf_viewer.css";

pdfjs.GlobalWorkerOptions.workerSrc = workerSrc;

export const Tool = {
  none: pdfjs.AnnotationEditorType.NONE,
  text: pdfjs.AnnotationEditorType.FREETEXT,
  draw: pdfjs.AnnotationEditorType.INK,
  highlight: pdfjs.AnnotationEditorType.HIGHLIGHT,
} as const;
export type ToolName = keyof typeof Tool;

const Param = pdfjs.AnnotationEditorParamsType;
const resource = (dir: string) => new URL(`/pdfjs/${dir}/`, location.href).href;

/** Asked for a password; `retry` after a wrong one. Resolves null to give up. */
export type AskPassword = (retry: boolean) => Promise<string | null>;

interface UiManager {
  undo(): void;
  redo(): void;
  updateParams(type: number, value: unknown): void;
}

export class PdfView {
  readonly bus = new EventBus();
  readonly viewer: PDFViewer;
  doc: PDFDocumentProxy | null = null;
  canUndo = false;
  private links: PDFLinkService;
  private ui: UiManager | null = null;

  constructor(container: HTMLDivElement, onState: () => void) {
    this.links = new PDFLinkService({
      eventBus: this.bus,
      externalLinkTarget: LinkTarget.BLANK,
      externalLinkRel: "noopener noreferrer nofollow",
    });
    const findController = new PDFFindController({ eventBus: this.bus, linkService: this.links });
    this.viewer = new PDFViewer({
      container,
      eventBus: this.bus,
      linkService: this.links,
      findController,
      annotationMode: pdfjs.AnnotationMode.ENABLE_FORMS,
      annotationEditorMode: Tool.none,
    });
    this.links.setViewer(this.viewer);
    this.bus.on("annotationeditoruimanager", ({ uiManager }: { uiManager: UiManager }) => (this.ui = uiManager));
    this.bus.on("annotationeditorstateschanged", ({ details }: { details: { hasSomethingToUndo: boolean } }) => {
      this.canUndo = details.hasSomethingToUndo;
      onState();
    });
    for (const ev of ["pagechanging", "scalechanging", "pagesinit", "annotationeditormodechanged"]) this.bus.on(ev, onState);
  }

  /** Opens bytes, keeping the page and zoom when reopening an edited copy. */
  async open(bytes: Uint8Array, ask: AskPassword, onModified: () => void): Promise<void> {
    const keep = this.doc ? { page: this.viewer.currentPageNumber, scale: this.viewer.currentScaleValue } : null;
    const task = pdfjs.getDocument({
      data: bytes.slice(), // pdf.js takes ownership of the buffer
      cMapUrl: resource("cmaps"),
      cMapPacked: true,
      standardFontDataUrl: resource("standard_fonts"),
      wasmUrl: resource("wasm"),
      iccUrl: resource("iccs"),
      enableXfa: false,
    });
    task.onPassword = async (update: (pw: string) => void, reason: number) => {
      const pw = await ask(reason === pdfjs.PasswordResponses.INCORRECT_PASSWORD);
      if (pw === null) void task.destroy();
      else update(pw);
    };
    const doc = await task.promise;
    const old = this.doc;
    this.doc = doc;
    this.canUndo = false;
    (doc.annotationStorage as { onSetModified: (() => void) | null }).onSetModified = onModified;
    this.bus.on(
      "pagesinit",
      () => {
        this.viewer.currentScaleValue = keep?.scale ?? "auto";
        if (keep) this.viewer.currentPageNumber = Math.min(keep.page, doc.numPages);
      },
      { once: true },
    );
    this.viewer.setDocument(doc);
    this.links.setDocument(doc);
    void old?.loadingTask.destroy();
  }

  /** The file with every form value and annotation written in. */
  async bytes(): Promise<Uint8Array> {
    if (!this.doc) throw new Error("No document");
    return this.doc.annotationStorage.size > 0 ? this.doc.saveDocument() : this.doc.getData();
  }

  set tool(name: ToolName) {
    if (this.doc) this.viewer.annotationEditorMode = { mode: Tool[name] };
  }

  get tool(): ToolName {
    // The getter returns the bare mode number; the typings mirror the setter's object.
    const mode = this.viewer.annotationEditorMode as unknown as number;
    return (Object.keys(Tool) as ToolName[]).find((k) => Tool[k] === mode) ?? "none";
  }

  /** Colour for new ink, text and highlights. */
  setColor(hex: string) {
    for (const p of [Param.INK_COLOR, Param.FREETEXT_COLOR, Param.HIGHLIGHT_COLOR]) this.ui?.updateParams(p, hex);
  }

  undo() {
    this.ui?.undo();
  }

  zoom(factor: number) {
    this.viewer.currentScale = Math.min(8, Math.max(0.25, this.viewer.currentScale * factor));
  }
}
