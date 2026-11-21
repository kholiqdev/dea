// pdf.js, loaded only when a PDF preview opens: it is far too big for the
// first screen, so nothing may import it statically.

import type { PDFDocumentProxy } from "pdfjs-dist";

export interface PdfDocument {
  numPages: number;
  getPage: PDFDocumentProxy["getPage"];
  /** Frees the document and stops its worker. */
  destroy: () => Promise<void>;
}

export async function loadPdf(bytes: ArrayBuffer): Promise<PdfDocument> {
  const [pdfjs, { default: workerUrl }] = await Promise.all([
    import("pdfjs-dist"),
    import("pdfjs-dist/build/pdf.worker.min.mjs?url"),
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
