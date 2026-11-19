<!-- A still picture of a tab bar, an editor and a terminal in the chosen
     theme, fonts and size. -->
<script lang="ts">
  import ArrowLeftRight from "@lucide/svelte/icons/arrow-left-right";
  import Database from "@lucide/svelte/icons/database";

  type Token = [kind: string, text: string];

  const EDITOR: Token[][] = [
    [["punct", "{"]],
    [
      ["", "  "],
      ["key", '"id"'],
      ["punct", ": "],
      ["str", '"shp_4821"'],
      ["punct", ","],
    ],
    [
      ["", "  "],
      ["key", '"weight"'],
      ["punct", ": "],
      ["num", "12.5"],
      ["punct", ","],
    ],
    [
      ["", "  "],
      ["key", '"insured"'],
      ["punct", ": "],
      ["bool", "true"],
    ],
    [["punct", "}"]],
  ];

  const TERMINAL: Token[][] = [
    [
      ["prompt", "deploy@sg-bastion:~$"],
      ["", " tail -n 2 app.log"],
    ],
    [
      ["muted", "12:04:31"],
      ["", " GET /shipments/4821 200 18ms"],
    ],
    [
      ["muted", "12:04:33"],
      ["", " POST /shipments 201 42ms"],
    ],
  ];
</script>

<figure class="card" aria-label="Preview">
  <div class="tabbar" aria-hidden="true">
    <div class="tab active">
      <ArrowLeftRight size={16} strokeWidth={1.5} />Shipment by ID<span
        class="env">staging</span
      >
    </div>
    <div class="tab">
      <Database size={16} strokeWidth={1.5} />harbor_production<span
        class="env prod">production</span
      >
    </div>
  </div>
  {#each [["editor", EDITOR] as const, ["terminal", TERMINAL] as const] as [name, lines] (name)}
    <div class={name}>
      {#each lines as line, i (i)}
        <div class="line">
          {#each line as [kind, text], j (j)}<span class={kind}>{text}</span
            >{/each}
        </div>
      {/each}
    </div>
  {/each}
</figure>

<style>
  .card {
    margin: 0;
    overflow: hidden;
    border-radius: var(--r-card);
    background: var(--bg-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .tabbar {
    height: var(--h-tabs);
    display: flex;
    background: var(--bg-0);
    box-shadow: inset 0 -1px 0 var(--line);
  }

  .tab {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    padding: 0 var(--s-3);
    border-right: 1px solid var(--line);
    color: var(--text-2);
    white-space: nowrap;
  }

  .tab.active {
    background: var(--bg-1);
    color: var(--text-1);
  }

  .env {
    padding: 0 6px;
    height: 18px;
    display: inline-flex;
    align-items: center;
    border-radius: 4px;
    font: 400 11px/1 var(--font-mono);
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--line);
  }

  .env.prod {
    color: var(--negative-text);
    background: color-mix(in srgb, var(--negative) 6%, transparent);
    box-shadow: inset 0 0 0 1px var(--negative);
  }

  .editor,
  .terminal {
    margin: 0;
    padding: var(--s-3) var(--s-4);
    font-family: var(--font-mono);
    font-size: var(--font-size, 13px);
    line-height: 1.55;
    white-space: pre;
    overflow: hidden;
  }

  .line {
    min-height: 1lh;
  }

  .editor {
    background: var(--bg-1);
    color: var(--syn-punct);
  }

  .terminal {
    background: var(--bg-0);
    color: var(--text-1);
    border-top: 1px solid var(--line);
  }

  .key {
    color: var(--syn-key);
  }

  .str {
    color: var(--syn-str);
  }

  .num {
    color: var(--syn-num);
  }

  .bool {
    color: var(--syn-bool);
  }

  .punct {
    color: var(--syn-punct);
  }

  .prompt {
    color: var(--positive-text);
  }

  .muted {
    color: var(--text-3);
  }
</style>
