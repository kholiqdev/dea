import { fileURLToPath, URL } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { configDefaults, defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  // The DOM cleanup between tests is listed in setupFiles below: the
  // plugin's own does not reach the projects.
  plugins: [svelte(), svelteTesting({ autoCleanup: false })],
  resolve: {
    alias: {
      $app: fileURLToPath(new URL("./src/app", import.meta.url)),
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 5174 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
  },
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
