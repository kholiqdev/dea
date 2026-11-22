<!--
  The URL the request goes to, under the request bar: secrets hidden, the
  host a step brighter so the environment reads at a glance, and names with
  no value marked as in the URL field. A click copies what it shows.
-->
<script lang="ts">
  import { variableRanges } from "@dea/editor";
  import { shellApi } from "$app/contributions/shell-api";
  import { copyText } from "$lib/ipc/clipboard";
  import { errorMessage } from "$lib/ipc/errors";

  let {
    url,
    undefinedNames,
  }: { url: string | null; undefinedNames: string[] } = $props();

  interface Piece {
    text: string;
    host: boolean;
    undefined: boolean;
  }

  // `{{ vault: token }}` is listed as `vault:token`.
  const key = (name: string) => name.replace(/^vault:\s*/, "vault:");

  /** Where the host is: after `://`, up to the first `/`, `?` or `#`. */
  function hostRange(text: string): { from: number; to: number } | null {
    const scheme = text.indexOf("://");
    if (scheme < 0) return null;
    const from = scheme + 3;
    const end = text.slice(from).search(/[/?#]/);
    return { from, to: end < 0 ? text.length : from + end };
  }

  /** The URL cut where the host and each name with no value start and end. */
  const pieces = $derived.by((): Piece[] => {
    if (!url) return [];
    const text = url;
    const missing = undefinedNames.map(key);
    const marked = variableRanges(text).filter((r) =>
      missing.includes(key(r.name)),
    );
    const host = hostRange(text);
    const cuts = [0, text.length]
      .concat(host ? [host.from, host.to] : [])
      .concat(marked.flatMap((r) => [r.from, r.to]))
      .sort((a, b) => a - b)
      .filter((at, i, all) => at !== all[i - 1]);
    return cuts.slice(1).map((to, i) => {
      const from = cuts[i] ?? 0;
      return {
        text: text.slice(from, to),
        host: host !== null && from >= host.from && to <= host.to,
        undefined: marked.some((r) => from >= r.from && to <= r.to),
      };
    });
  });

  async function copy(text: string): Promise<void> {
    try {
      await copyText(text);
      shellApi().toast("URL copied");
    } catch (e) {
      shellApi().toast(errorMessage(e));
    }
  }
</script>

<div class="resolved">
  {#if url}
    {@const text = url}
    <!-- Its name is what it does and the URL it copies. -->
    <button
      type="button"
      class="copy"
      title={text}
      data-final-url=""
      onclick={() => void copy(text)}
    >
      <span class="visually-hidden">Copy the final URL,</span>
      <span class="arrow" aria-hidden="true">→</span>
      <span class="url"
        >{#each pieces as piece, i (i)}{#if piece.host || piece.undefined}<span
              class={{ host: piece.host, undefined: piece.undefined }}
              >{piece.text}</span
            >{:else}{piece.text}{/if}{/each}</span
      >
    </button>
  {/if}
</div>

<style>
  .resolved {
    height: 20px;
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 var(--s-4);
  }

  .copy {
    max-width: 100%;
    min-width: 0;
    height: 20px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    border: 0;
    border-radius: var(--r-key);
    background: none;
    font: 400 12px/20px var(--font-mono);
    color: var(--text-3);
  }

  .copy:hover {
    color: var(--text-2);
  }

  .arrow {
    flex: none;
  }

  .url {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .host {
    color: var(--text-2);
  }

  .copy:hover .host {
    color: var(--text-1);
  }

  /* As in the URL field: not by colour alone (WCAG 1.4.1). */
  .undefined,
  .copy:hover .undefined {
    color: var(--negative-text);
    text-decoration: underline wavy var(--negative-text);
    text-underline-offset: 2px;
  }
</style>
