import type { TimingInfo } from "$lib/ipc/bindings/TimingInfo";

/** A phase of the time a response took, with the colour that marks it. */
export interface TimingPhase {
  id: "dns" | "connect" | "tls" | "waiting" | "download";
  label: string;
  ms: number;
  color: string;
}

/** The phases of `timing`, in the order they happen. */
export function timingPhases(timing: TimingInfo): TimingPhase[] {
  return [
    { id: "dns", label: "DNS", ms: timing.dnsMs, color: "var(--series-1)" },
    {
      id: "connect",
      label: "Connect",
      ms: timing.connectMs,
      color: "var(--series-2)",
    },
    { id: "tls", label: "TLS", ms: timing.tlsMs, color: "var(--series-3)" },
    {
      id: "waiting",
      label: "Waiting for server",
      ms: timing.waitingMs,
      color: "var(--accent)",
    },
    {
      id: "download",
      label: "Download",
      ms: timing.downloadMs,
      color: "var(--series-4)",
    },
  ];
}
