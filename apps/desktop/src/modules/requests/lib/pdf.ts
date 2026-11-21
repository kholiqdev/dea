// pdf.js, loaded only when a PDF preview opens: it is far too big for the
// first screen, so nothing may import it statically. It comes from the
// legacy build: the modern one needs newer JavaScript than WebView2 131.

import type { PDFDocumentProxy } from "pdfjs-dist/legacy/build/pdf.mjs";

export interface PdfDocument {
  numPages: number;
  getPage: PDFDocumentProxy["getPage"];
  /** Frees the document and stops its worker. */
  destroy: () => Promise<void>;
}

export async function loadPdf(bytes: ArrayBuffer): Promise<PdfDocument> {
  const [pdfjs, { default: workerUrl }] = await Promise.all([
    import("pdfjs-dist/legacy/build/pdf.mjs"),
    import("pdfjs-dist/legacy/build/pdf.worker.min.mjs?url"),
  ]);
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
  const task = pdfjs.getDocument({ data: new Uint8Array(bytes) });
  const doc = await task.promise;
  return {
    numPages: doc.numPages,
    getPage: (n) => doc.getPage(n),
    destroy: () => task.destroy(),
  };
}
