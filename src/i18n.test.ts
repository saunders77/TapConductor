// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

import {
  LANGUAGE_OPTIONS,
  SUPPORTED_LOCALES,
  createLocalizer,
  hasMessageKey,
  hasTranslationFor,
  localizedEmptyStateHtml,
  localizedHelpHtml,
  resolveLocale,
} from "./i18n.ts";

test("exposes twelve distinct App Store languages plus the system preference", () => {
  assert.equal(SUPPORTED_LOCALES.length, 12);
  assert.equal(new Set(SUPPORTED_LOCALES).size, 12);
  assert.deepEqual(LANGUAGE_OPTIONS.map(({ value }) => value), ["system", ...SUPPORTED_LOCALES]);
});

test("system locale matching understands region tags and falls back to English", () => {
  assert.equal(resolveLocale("system", ["fr-CA", "en-US"]), "fr");
  assert.equal(resolveLocale("system", ["zh-CN"]), "zh-Hans");
  assert.equal(resolveLocale("system", ["pt-PT"]), "pt-BR");
  assert.equal(resolveLocale("system", ["nl-NL"]), "en");
  assert.equal(resolveLocale("ja", ["fr-FR"]), "ja");
});

test("every supported non-English locale has localized functional Info and empty-state markup", () => {
  for (const locale of SUPPORTED_LOCALES) {
    const { t } = createLocalizer(locale);
    assert.notEqual(t("openScore"), "openScore");
    if (locale === "en") continue;
    const help = localizedHelpHtml(locale, t) ?? "";
    const empty = localizedEmptyStateHtml(locale, t) ?? "";
    for (const id of [
      "help-demo-choir-open", "help-demo-piano-open", "piano-shortcut-pitch",
      "telemetry-settings-link", "empty-open", "demo-choir-open", "demo-piano-open",
    ]) {
      assert.match(`${help}${empty}`, new RegExp(`id="${id}"`));
    }
    assert.doesNotMatch(`${help}${empty}`, /undefined|>openScore<|>demoChoir</);
  }
});

test("message formatting localizes interpolated values", () => {
  assert.equal(createLocalizer("de").t("measure", { number: 12 }), "Takt 12");
  assert.equal(createLocalizer("ar").t("partsCount", { enabled: 2, total: 4 }), "2 من 4");
});

test("every message key used by the UI exists in the complete catalog", () => {
  const source = ["main.ts", "i18n.ts"]
    .map((name) => readFileSync(new URL(`./${name}`, import.meta.url), "utf8"))
    .join("\n");
  const keys = [...source.matchAll(/\bt\("([^"]+)"/g)].map((match) => match[1]!);
  assert.deepEqual(keys.filter((key) => !hasMessageKey(key)), []);
});

test("every app-authored phrase in persistent markup has a translation", () => {
  const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
  let markup = source.match(/app\.innerHTML = `([\s\S]*?)`;\n/)?.[1] ?? "";
  // These two regions are replaced by locale-specific structured content before display.
  markup = markup
    .replace(/<div class="help-content">[\s\S]*?<\/div>\s*<button id="help-done"/, '<button id="help-done"')
    .replace(/<div id="empty-state"[\s\S]*?<p id="score-keyboard-help"/, '<p id="score-keyboard-help"');
  const phrases = [
    ...[...markup.matchAll(/>([^<>]+)</g)].map((match) => match[1]!.replace(/\s+/g, " ").trim()),
    ...[...markup.matchAll(/(?:aria-label|title)="([^"]+)"/g)].map((match) => match[1]!),
  ].filter((value) => /[A-Za-z]/.test(value));
  const intentionallyInvariant = new Set(["Tap", "Conductor", "TapConductor", "0 ms", "120 ms"]);
  assert.deepEqual(
    [...new Set(phrases)].filter((phrase) => !intentionallyInvariant.has(phrase) && !hasTranslationFor(phrase)),
    [],
  );
});

test("Apple bundles declare and package every supported localization", () => {
  const root = new URL("../src-tauri/", import.meta.url);
  for (const configName of ["tauri.ios.conf.json", "tauri.macos.conf.json"]) {
    const config = readFileSync(new URL(configName, root), "utf8");
    for (const locale of SUPPORTED_LOCALES) {
      assert.match(config, new RegExp(`apple/locales/${locale.replace("-", "\\-")}\\.lproj`));
    }
  }
  for (const plistName of ["Info.ios.plist", "Info.macos.plist"]) {
    const plist = readFileSync(new URL(`apple/${plistName}`, root), "utf8");
    for (const locale of SUPPORTED_LOCALES) assert.match(plist, new RegExp(`<string>${locale}</string>`));
  }
  for (const locale of SUPPORTED_LOCALES) {
    assert.equal(existsSync(new URL(`apple/locales/${locale}.lproj/InfoPlist.strings`, root)), true);
  }
});
