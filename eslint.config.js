import { fileURLToPath } from "node:url";
import js from "@eslint/js";
import boundaries from "eslint-plugin-boundaries";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import ts from "typescript-eslint";
import svelteConfig from "./apps/desktop/svelte.config.js";

const desktopRoot = fileURLToPath(new URL("./apps/desktop", import.meta.url));

export default ts.config(
  {
    ignores: [
      "**/dist/**",
      "**/node_modules/**",
      "**/target/**",
      "**/src-tauri/gen/**",
    ],
  },
  js.configs.recommended,
  // Type-aware rules catch unhandled promises (for example a failed Tauri
  // `invoke`) and unsafe `any`.
  ...ts.configs.recommendedTypeChecked,
  ...svelte.configs.recommended,
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: ["**/*.svelte", "**/*.svelte.ts"],
    languageOptions: {
      parserOptions: {
        parser: ts.parser,
        projectService: true,
        extraFileExtensions: [".svelte"],
        svelteConfig,
      },
    },
  },
  {
    // Config files are plain JavaScript outside every TypeScript project.
    files: ["**/*.js"],
    ...ts.configs.disableTypeChecked,
  },
  {
    files: ["apps/desktop/src/**/*.{ts,svelte}"],
    plugins: { boundaries },
    settings: {
      "boundaries/root-path": desktopRoot,
      "import/resolver": {
        typescript: { project: `${desktopRoot}/tsconfig.app.json` },
      },
      "boundaries/elements": [
        { type: "shell-api", pattern: "src/app/contributions" },
        { type: "app", pattern: "src/app" },
        { type: "module", pattern: "src/modules/*" },
        { type: "lib", pattern: "src/lib" },
      ],
    },
    rules: {
      "boundaries/dependencies": [
        "error",
        {
          default: "disallow",
          policies: [
            {
              from: { element: { type: "app" } },
              allow: {
                to: { element: { types: ["app", "shell-api", "lib"] } },
              },
            },
            {
              // The shell only sees a module's manifest, so views stay lazy.
              from: { element: { type: "app" } },
              allow: {
                to: {
                  element: { type: "module", fileInternalPath: "index.ts" },
                },
              },
            },
            {
              from: { element: { type: "shell-api" } },
              allow: { to: { element: { types: ["shell-api", "lib"] } } },
            },
            {
              from: { element: { type: "module" } },
              allow: { to: { element: { types: ["shell-api", "lib"] } } },
            },
            {
              from: { element: { type: "lib" } },
              allow: { to: { element: { type: "lib" } } },
            },
          ],
        },
      ],
    },
  },
  {
    // The shell loads at startup, so the code editor must stay out of it.
    files: ["apps/desktop/src/app/**/*.{ts,svelte}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@dea/editor", "@dea/editor/*"],
              message: "Only modules may import @dea/editor.",
            },
          ],
        },
      ],
    },
  },
  {
    files: ["apps/desktop/src/lib/**/*.{ts,svelte}"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          patterns: [
            {
              group: ["@dea/ui", "@dea/ui/*", "@dea/editor", "@dea/editor/*"],
              message: "src/lib holds UI-free utilities.",
            },
          ],
        },
      ],
    },
  },
);
