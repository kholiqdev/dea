/** View choices kept for the session only; they are never saved. */
export const sessionView = $state({
  /** Whether the tables show their Description column. */
  descriptions: false,
  /** Whether the response timing legend is open. */
  timingLegend: false,
});
