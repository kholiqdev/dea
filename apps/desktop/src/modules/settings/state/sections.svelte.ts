// Which Settings section is open. The sidebar and the main view share it.

export const sections = $state<{ current: string | null }>({ current: null });
