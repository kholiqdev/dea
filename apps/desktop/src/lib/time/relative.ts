// Times as the design writes them: "8 min ago", "Expires in 3 h".

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const MONTH = 30 * DAY;

function plural(n: number, one: string, many: string): string {
  return n === 1 ? `1 ${one}` : `${n} ${many}`;
}

/** How long ago `iso` was; a time in the future reads as just now. */
export function relativeTime(iso: string, now: Date): string {
  const ms = now.getTime() - new Date(iso).getTime();
  if (ms < MIN) return "just now";
  if (ms < HOUR) return `${Math.floor(ms / MIN)} min ago`;
  if (ms < DAY) return `${plural(Math.floor(ms / HOUR), "hour", "hours")} ago`;
  if (ms < MONTH) return `${plural(Math.floor(ms / DAY), "day", "days")} ago`;
  return `${plural(Math.floor(ms / MONTH), "month", "months")} ago`;
}

/** How long `iso` is away, or `Expired` once it has passed. */
export function expiresIn(iso: string, now: Date): string {
  const ms = new Date(iso).getTime() - now.getTime();
  if (ms <= 0) return "Expired";
  if (ms < HOUR) return `Expires in ${Math.max(1, Math.floor(ms / MIN))} min`;
  if (ms < DAY) return `Expires in ${Math.floor(ms / HOUR)} h`;
  return `Expires in ${plural(Math.floor(ms / DAY), "day", "days")}`;
}
