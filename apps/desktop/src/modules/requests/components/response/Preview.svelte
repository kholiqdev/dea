<!--
  A response shown as what it is: HTML in a sandbox without script, an
  image from a blob URL, a PDF drawn by pdf.js.
-->
<script lang="ts">
  import { errorMessage } from "$lib/ipc/errors";
  import { responseBytes, responseText } from "$lib/ipc/responses";

  let {
    id,
    kind,
    contentType,
  }: {
    id: string;
    kind: string;
    contentType: string | null;
  } = $props();

  let html = $state<string | null>(null);
  let image = $state<{ url: string; width?: number; height?: number } | null>(
    null,
  );
  let error = $state<string | null>(null);

  $effect(() => {
    const at = id;
    html = null;
    image = null;
    error = null;
    if (kind === "html") {
      let live = true;
      responseText(at, "raw")
        .then((body) => {
          if (live) html = body.text;
        })
        .catch((e: unknown) => {
          if (live) error = errorMessage(e);
        });
      return () => (live = false);
    }
    if (kind === "image") {
      let live = true;
      let url: string | undefined;
      responseBytes(at)
        .then((bytes) => {
          if (!live) return;
          url = URL.createObjectURL(
            new Blob([bytes], { type: contentType ?? "" }),
          );
          image = { url };
        })
        .catch((e: unknown) => {
          if (live) error = errorMessage(e);
        });
      return () => {
        live = false;
        if (url) URL.revokeObjectURL(url);
      };
    }
    return undefined;
  });

  function measured(e: Event): void {
    const img = e.currentTarget as HTMLImageElement;
    if (image) {
      image = { ...image, width: img.naturalWidth, height: img.naturalHeight };
    }
  }
</script>

{#if error !== null}
  <p class="error" role="alert">The preview could not be shown: {error}</p>
{:else if kind === "html"}
  {#if html !== null}
    <iframe title="Preview of the response" sandbox="" srcdoc={html}></iframe>
  {/if}
{:else if kind === "image"}
  {#if image}
    <div class="image">
      <!-- svelte-ignore a11y_img_redundant_alt -->
      <img src={image.url} alt="Response image" onload={measured} />
      {#if image.width !== undefined}
        <p class="caption">{image.width} × {image.height}</p>
      {/if}
    </div>
  {/if}
{:else if kind === "pdf"}
  {#await import("./PdfPreview.svelte") then { default: PdfPreview }}
    <PdfPreview {id} />
  {/await}
{/if}

<style>
  iframe {
    flex: 1;
    min-height: 0;
    border: 0;
    background: var(--bg-1);
  }

  .image {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: var(--s-4);
    display: flex;
    flex-direction: column;
    gap: var(--s-2);
  }

  img {
    align-self: flex-start;
    max-width: 100%;
  }

  .caption {
    margin: 0;
    color: var(--text-3);
  }

  .error {
    margin: 0;
    padding: var(--s-4);
    color: var(--negative-text);
  }
</style>
