// Mod+S in the environment editor.

import { environmentsEditor } from "../state/environments.svelte";

export default function saveEnvironments(): Promise<void> {
  return environmentsEditor().save();
}
