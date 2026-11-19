// @vitest-environment node
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { createGenerator } from "ts-json-schema-generator";
import { expect, test } from "vitest";

const here = import.meta.dirname;

test("the published manifest schema matches the types", () => {
  const generated = createGenerator({
    path: join(here, "manifest-data.ts"),
    tsconfig: join(here, "../../../tsconfig.app.json"),
    type: "ManifestData",
    skipTypeCheck: true,
  }).createSchema("ManifestData");
  const published: unknown = JSON.parse(
    readFileSync(join(here, "manifest.schema.json"), "utf8"),
  );
  expect(
    published,
    "manifest.schema.json is stale: run `pnpm --filter desktop schema:manifest`",
  ).toEqual(JSON.parse(JSON.stringify(generated)));
}, 30_000);
