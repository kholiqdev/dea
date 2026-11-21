import { render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { fakeRequestsIpc, fakeResponse } from "../../test-support";
import Preview from "./Preview.svelte";

const pdf = vi.hoisted(() => ({
  loadPdf: vi.fn(),
  loaded: false,
}));
vi.mock("../../lib/pdf", () => {
  pdf.loaded = true;
  return { loadPdf: pdf.loadPdf };
});

beforeEach(() => {
  pdf.loadPdf.mockReset();
});
afterEach(() => vi.unstubAllGlobals());

test("html_preview_is_sandboxed_without_scripts", async () => {
  fakeRequestsIpc({
    response: fakeResponse({
      text: { raw: "<p>hi</p><script>parent.x=1</script>" },
    }),
  });
  render(Preview, { id: "t1", kind: "html", contentType: "text/html" });
  const frame = await screen.findByTitle("Preview of the response");
  expect(frame.getAttribute("sandbox")).toBe("");
  expect(frame.getAttribute("srcdoc")).toContain("<p>hi</p>");
});

test("image_preview_uses_a_blob_url_and_releases_it", async () => {
  const create = vi.fn(() => "blob:dea/1");
  const revoke = vi.fn();
  vi.stubGlobal(
    "URL",
    Object.assign(URL, {
      createObjectURL: create,
      revokeObjectURL: revoke,
    }),
  );
  fakeRequestsIpc({
    response: fakeResponse({ bytes: new Uint8Array([1, 2, 3]) }),
  });
  const { unmount } = render(Preview, {
    id: "t1",
    kind: "image",
    contentType: "image/png",
  });
  const img = await screen.findByAltText("Response image");
  expect(img.getAttribute("src")).toBe("blob:dea/1");
  const blob = (create.mock.calls[0] as unknown as [Blob])[0];
  expect(blob.type).toBe("image/png");
  Object.defineProperty(img, "naturalWidth", { value: 40 });
  Object.defineProperty(img, "naturalHeight", { value: 30 });
  img.dispatchEvent(new Event("load"));
  expect(await screen.findByText("40 × 30")).toBeInTheDocument();
  unmount();
  expect(revoke).toHaveBeenCalledWith("blob:dea/1");
});

test("pdf_preview_loads_pdfjs_only_when_shown", async () => {
  fakeRequestsIpc({ response: fakeResponse({ bytes: new Uint8Array([1]) }) });
  pdf.loadPdf.mockRejectedValue(new Error("x"));
  expect(pdf.loaded).toBe(false);
  render(Preview, { id: "t1", kind: "pdf", contentType: "application/pdf" });
  await waitFor(() => expect(pdf.loadPdf).toHaveBeenCalled());
  expect(pdf.loaded).toBe(true);
});

function fakeDocument(count: number) {
  const render = vi.fn(() => ({ promise: Promise.resolve() }));
  const destroy = vi.fn(() => Promise.resolve());
  return {
    render,
    destroy,
    doc: {
      numPages: count,
      destroy,
      getPage: () =>
        Promise.resolve({
          getViewport: ({ scale }: { scale: number }) => ({
            width: 100 * scale,
            height: 150 * scale,
          }),
          render,
        }),
    },
  };
}

test("pdf_preview_draws_each_page", async () => {
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    {} as CanvasRenderingContext2D,
  );
  const fake = fakeDocument(2);
  pdf.loadPdf.mockResolvedValue(fake.doc);
  fakeRequestsIpc({ response: fakeResponse({ bytes: new Uint8Array([1]) }) });
  const { unmount } = render(Preview, {
    id: "t1",
    kind: "pdf",
    contentType: "application/pdf",
  });
  expect(await screen.findByText("2 pages")).toBeInTheDocument();
  await waitFor(() => expect(fake.render).toHaveBeenCalledTimes(2));
  expect(document.querySelectorAll("canvas")).toHaveLength(2);
  unmount();
  expect(fake.destroy).toHaveBeenCalled();
});

test("a_pdf_that_fails_says_so", async () => {
  pdf.loadPdf.mockRejectedValue(new Error("bad"));
  fakeRequestsIpc({ response: fakeResponse({ bytes: new Uint8Array([1]) }) });
  render(Preview, { id: "t1", kind: "pdf", contentType: "application/pdf" });
  expect(
    await screen.findByText(
      "This PDF could not be shown. Save it to a file to open it.",
    ),
  ).toBeInTheDocument();
});

test("a_pdf_that_finishes_loading_after_unmount_is_destroyed", async () => {
  const fake = fakeDocument(1);
  let finish: (doc: typeof fake.doc) => void = () => {};
  pdf.loadPdf.mockReturnValue(new Promise((resolve) => (finish = resolve)));
  fakeRequestsIpc({ response: fakeResponse({ bytes: new Uint8Array([1]) }) });
  const { unmount } = render(Preview, {
    id: "t1",
    kind: "pdf",
    contentType: "application/pdf",
  });
  await waitFor(() => expect(pdf.loadPdf).toHaveBeenCalled());
  unmount();
  finish(fake.doc);
  await waitFor(() => expect(fake.destroy).toHaveBeenCalledTimes(1));
  expect(fake.render).not.toHaveBeenCalled();
});

test("a_page_without_a_canvas_context_fails_instead_of_staying_blank", async () => {
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  pdf.loadPdf.mockResolvedValue(fakeDocument(1).doc);
  fakeRequestsIpc({ response: fakeResponse({ bytes: new Uint8Array([1]) }) });
  render(Preview, { id: "t1", kind: "pdf", contentType: "application/pdf" });
  expect(
    await screen.findByText(
      "This PDF could not be shown. Save it to a file to open it.",
    ),
  ).toBeInTheDocument();
});
