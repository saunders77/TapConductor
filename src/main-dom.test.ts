// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

test("every required UI element is present in the application markup", () => {
  const markupIds = new Set(
    [...source.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]!),
  );
  const requiredIds = [...source.matchAll(/byId(?:<[^;\n]+?>)?\("([^"]+)"\)/g)]
    .map((match) => match[1]!);

  assert.deepEqual(requiredIds.filter((id) => !markupIds.has(id)), []);
});

