// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { groupScoreWarnings, warningContext } from "./score-warning-groups.ts";
import type { ImportWarningDto } from "./types.ts";

function warning(code: ImportWarningDto["code"], message: string, measureId?: string): ImportWarningDto {
  return { code, severity: "warning", message, context: { measureId } };
}

test("three or more score warnings of one type become one expandable group", () => {
  const warnings = [
    warning("overfullMeasure", "actual extent 5 exceeds 4", "1"),
    warning("missingPitch", "missing pitch"),
    warning("overfullMeasure", "actual extent 6 exceeds 4", "2"),
    warning("overfullMeasure", "actual extent 7 exceeds 4", "3"),
  ];

  const items = groupScoreWarnings(warnings);

  assert.equal(items.length, 2);
  assert.equal(items[0]?.kind, "group");
  if (items[0]?.kind !== "group") return;
  assert.equal(items[0].group.description, "note durations exceed bar lengths");
  assert.deepEqual(items[0].group.warnings, [warnings[0], warnings[2], warnings[3]]);
  assert.deepEqual(items[1], { kind: "single", warning: warnings[1] });
});

test("one or two warnings of a type remain individual messages", () => {
  const warnings = [warning("unterminatedTie", "first"), warning("unterminatedTie", "second")];
  assert.deepEqual(groupScoreWarnings(warnings), warnings.map((item) => ({ kind: "single", warning: item })));
});

test("warning context prefers the score's measure label", () => {
  const item = warning("overfullMeasure", "message", "12a");
  item.context.partId = "Piano";
  item.context.measureIndex = 11;
  assert.equal(warningContext(item), "Part Piano, measure 12a");
});
