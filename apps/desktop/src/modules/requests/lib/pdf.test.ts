import { expect, test, vi } from "vitest";
import { loadPdf } from "./pdf";

// The legacy build: the modern one needs newer JavaScript than WebView2 131.
const pdfjs = vi.hoisted(() => ({
  GlobalWorkerOptions: { workerSrc: "" },
  getDocument: vi.fn(),
}));
vi.mock("pdfjs-dist/legacy/build/pdf.mjs", () => pdfjs);
vi.mock("pdfjs-dist/legacy/build/pdf.worker.min.mjs?url", () => ({
  default: "/assets/legacy-worker.mjs",
}));

test("loads_pdfjs_and_its_worker_from_the_legacy_build", async () => {
  const doc = { numPages: 3, getPage: vi.fn() };
  const destroy = vi.fn(() => Promise.resolve());
  pdfjs.getDocument.mockReturnValue({ promise: Promise.resolve(doc), destroy });

  const loaded = await loadPdf(new Uint8Array([37, 80, 68, 70]).buffer);

  expect(pdfjs.GlobalWorkerOptions.workerSrc).toBe("/assets/legacy-worker.mjs");
  expect(pdfjs.getDocument).toHaveBeenCalledWith({
    data: new Uint8Array([37, 80, 68, 70]),
  });
  expect(loaded.numPages).toBe(3);
  await loaded.destroy();
  expect(destroy).toHaveBeenCalled();
});
