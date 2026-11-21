<!--
  A PDF drawn page by page onto canvases as wide as the pane. WebKitGTK has
  no PDF viewer and the CSP blocks `blob:` frames, so pdf.js draws it.
-->
<script lang="ts">
  import { tick } from "svelte";
  import { responseBytes } from "$lib/ipc/responses";
  import { loadPdf, type PdfDocument } from "../../lib/pdf";

  let { id }: { id: string } = $props();

  let pane = $state<HTMLElement>();
  let canvases = $state<HTMLCanvasElement[]>([]);
  let pages = $state(0);
  let failed = $state(false);

  $effect(() => {
    const at = id;
    let live = true;
    let doc: PdfDocument | undefined;
    pages = 0;
    failed = false;
    void (async () => {
      try {
        const loaded = await loadPdf(await responseBytes(at));
        if (!live) {
          // The preview closed while this was loading: nobody else will.
          void loaded.destroy();
          return;
        }
        doc = loaded;
        pages = doc.numPages;
        await tick();
        const width = pane?.clientWidth || 600;
        const ratio = window.devicePixelRatio || 1;
        for (let n = 1; n <= doc.numPages; n++) {
          const page = await doc.getPage(n);
          if (!live) return;
          const canvas = canvases[n - 1];
          const context = canvas?.getContext("2d");
          if (!canvas || !context) throw new Error("No canvas for the page");
          const base = page.getViewport({ scale: 1 });
          const viewport = page.getViewport({
            scale: (width / base.width) * ratio,
          });
          canvas.width = Math.floor(viewport.width);
          canvas.height = Math.floor(viewport.height);
          canvas.style.width = "100%";
          await page.render({ canvasContext: context, canvas, viewport })
            .promise;
        }
      } catch {
        if (live) failed = true;
      }
    })();
    return () => {
      live = false;
      void doc?.destroy();
    };
  });
</script>

{#if failed}
  <p class="failed" role="alert">
    This PDF could not be shown. Save it to a file to open it.
  </p>
{:else}
  <div class="pdf" bind:this={pane}>
    {#if pages > 0}
      <p class="caption">{pages} {pages === 1 ? "page" : "pages"}</p>
    {/if}
    {#each Array.from({ length: pages }, (_, n) => n) as n (n)}
      <canvas bind:this={canvases[n]} aria-label="Page {n + 1}"></canvas>
    {/each}
  </div>
{/if}

<style>
  .pdf {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
  }

  canvas {
    display: block;
    flex: none;
    max-width: 100%;
    background: var(--bg-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .caption {
    margin: 0;
    color: var(--text-3);
  }

  .failed {
    margin: 0;
    padding: var(--s-4);
    color: var(--negative-text);
  }
</style>
