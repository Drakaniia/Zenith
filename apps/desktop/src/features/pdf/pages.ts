// Edits that change the file itself, done with pdf-lib: page operations and placed signatures.
// Each takes the current bytes and returns new ones; the viewer then reloads them.

import { PDFDocument, degrees } from "pdf-lib";

export type PageOp =
  | { kind: "rotate"; index: number; by: 90 | -90 }
  | { kind: "delete"; index: number }
  | { kind: "move"; from: number; to: number }
  | { kind: "blank"; after: number }
  | { kind: "merge"; bytes: Uint8Array };

export interface Stamp {
  /** PNG image. */
  png: Uint8Array;
  page: number;
  /** Centre of the image, in PDF user space. */
  x: number;
  y: number;
  width: number;
  height: number;
}

const load = (bytes: Uint8Array) => PDFDocument.load(bytes, { updateMetadata: false });

export async function applyPageOp(bytes: Uint8Array, op: PageOp): Promise<Uint8Array> {
  const doc = await load(bytes);
  const count = doc.getPageCount();
  const at = (i: number) => {
    if (!Number.isInteger(i) || i < 0 || i >= count) throw new Error(`No page ${i + 1}`);
    return doc.getPage(i);
  };
  switch (op.kind) {
    case "rotate": {
      const page = at(op.index);
      page.setRotation(degrees((((page.getRotation().angle + op.by) % 360) + 360) % 360));
      break;
    }
    case "delete":
      at(op.index);
      if (count === 1) throw new Error("A PDF needs at least one page");
      doc.removePage(op.index);
      break;
    case "move": {
      const page = at(op.from);
      const to = Math.max(0, Math.min(count - 1, op.to));
      if (to === op.from) return bytes;
      doc.removePage(op.from);
      doc.insertPage(to, page);
      break;
    }
    case "blank": {
      const { width, height } = at(op.after).getSize();
      doc.insertPage(op.after + 1, [width, height]);
      break;
    }
    case "merge": {
      const other = await load(op.bytes);
      for (const p of await doc.copyPages(other, other.getPageIndices())) doc.addPage(p);
      break;
    }
  }
  return doc.save();
}

/** Lower-left corner for an image of `w`×`h` centred on (cx, cy) and turned `angle` degrees
 *  counter-clockwise about that corner (how pdf-lib rotates). */
export function cornerFor(cx: number, cy: number, w: number, h: number, angle: number) {
  const r = (angle * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return { x: cx - (cos * w - sin * h) / 2, y: cy - (sin * w + cos * h) / 2 };
}

/** Draws a signature image onto a page, upright as the page is displayed. */
export async function placeStamp(bytes: Uint8Array, s: Stamp): Promise<Uint8Array> {
  const doc = await load(bytes);
  const page = doc.getPage(s.page);
  const image = await doc.embedPng(s.png);
  const angle = page.getRotation().angle % 360;
  const { x, y } = cornerFor(s.x, s.y, s.width, s.height, angle);
  page.drawImage(image, { x, y, width: s.width, height: s.height, rotate: degrees(angle) });
  return doc.save();
}
