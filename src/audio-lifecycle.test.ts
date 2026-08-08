// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

test("a successful audio reconnection dismisses the inactive-audio error", () => {
  assert.match(
    source,
    /if \(diagnostics\.ready\) dismissErrorToast\(INACTIVE_AUDIO_ERROR\)/,
  );
  assert.match(source, /const displayedErrorToasts = new Map<string, HTMLElement>\(\)/);
  assert.match(source, /displayedErrorToasts\.set\(message, item\)/);
});

test("changing audio output releases the native selector before performance input resumes", () => {
  assert.match(
    source,
    /function releaseAudioOutputFocus\(\)[\s\S]*?elements\.audioOutput\.blur\(\)[\s\S]*?requestAnimationFrame[\s\S]*?document\.activeElement === elements\.audioOutput[\s\S]*?elements\.audioOutput\.blur\(\)/,
  );
  assert.match(
    source,
    /elements\.audioOutput\.addEventListener\("change", async \(\) => \{\s*releaseAudioOutputFocus\(\);/,
  );
});
