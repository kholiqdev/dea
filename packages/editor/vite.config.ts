import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  // The DOM cleanup between tests is listed in setupFiles below: the
  // plugin's own does not reach the projects.
  plugins: [svelte(), svelteTesting({ autoCleanup: false })],
  test: {
    environment: "jsdom",
    setupFiles: ["@testing-library/svelte/vitest", "./vitest.setup.ts"],
    projects: [
      {
        extends: true,
        test: {
          name: "unit",
          include: ["src/**/*.test.ts"],
          exclude: [...configDefaults.exclude, "src/**/*.perf.test.ts"],
        },
      },
      {
        // Tests with a time limit: one file at a time, after the unit
        // tests, so other workers do not slow them down.
        extends: true,
        test: {
          name: "perf",
          include: ["src/**/*.perf.test.ts"],
          fileParallelism: false,
          sequence: { groupOrder: 1 },
        },
      },
    ],
  },
});
