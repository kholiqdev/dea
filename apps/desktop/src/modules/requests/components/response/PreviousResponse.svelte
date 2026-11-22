<!--
  Marks the response shown as the previous one while a warning holds back
  the next send: when it came, and the environment it was sent in. Short of
  room the strip goes on to a second line between its parts, so the
  environment's name is never cut.
-->
<script lang="ts">
  import { formatClock } from "../../lib/format";

  let {
    receivedAt,
    environment,
  }: {
    /** The `Date.now()` at which the response came. */
    receivedAt: number;
    /** The environment it was sent in, if any. */
    environment: string | null;
  } = $props();
</script>

<!-- Each separator opens the part after it, so a line only breaks between
     parts. -->
<p class="previous">
  <span>Previous response</span><span>{` · ${formatClock(receivedAt)}`}</span
  >{#if environment !== null}<span>{` · ${environment}`}</span>{/if}
</p>

<style>
  /* A line of 28 px, as the timing legend's: a second one, short of room,
     is as tall. */
  .previous {
    flex: none;
    min-height: 28px;
    margin: 0;
    padding: 0 var(--s-4);
    display: flex;
    flex-wrap: wrap;
    background: var(--bg-2);
    box-shadow: inset 0 -1px 0 var(--line);
    font-size: 12px;
    line-height: 28px;
    color: var(--text-2);
  }

  /* Kept whole, and with the space that opens them: a flex item would
     otherwise drop it. */
  .previous span {
    white-space: pre;
  }
</style>
