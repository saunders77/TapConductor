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

test("iOS advertises every score type and drains system open requests after startup", () => {
  const plist = readFileSync(
    new URL("../src-tauri/apple/Info.ios.plist", import.meta.url),
    "utf8",
  );
  const nativeSource = readFileSync(
    new URL("../src-tauri/src/lib.rs", import.meta.url),
    "utf8",
  );

  for (const type of [
    "com.recordare.musicxml.uncompressed",
    "com.recordare.musicxml",
    "public.xml",
    "public.midi-audio",
  ]) {
    assert.match(plist, new RegExp(`<string>${type.replaceAll(".", "\\.")}</string>`));
  }
  assert.match(plist, /<key>LSSupportsOpeningDocumentsInPlace<\/key>\s*<false\/>/);
  assert.match(nativeSource, /tauri::RunEvent::Opened \{ urls \}/);
  assert.match(source, /listen<void>\("open-score-requested"[\s\S]*?loadPendingOpenedScores\(\)/);
  assert.match(source, /openedScoreHandlingReady = true;\s*void loadPendingOpenedScores\(\);/);
});

test("the empty score view stands alone and defers errors until a score opens", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(source, /<div class="shell score-empty">/);
  assert.match(source, /score = loaded;\s*shell\?\.classList\.remove\("score-empty"\);/);
  assert.match(styles, /\.shell\.score-empty \.topbar,[\s\S]*?\.shell\.score-empty \.performance-strip\s*{[^}]*display:\s*none\s*!important;/s);
  assert.match(styles, /\.shell\.score-empty \.workspace\s*{[^}]*grid-template-rows:\s*minmax\(0, 1fr\)\s*!important;/s);
  assert.match(styles, /\.shell\.score-empty \.toast\.error,[\s\S]*?\.shell\.score-empty \.grouped-warning\s*{[^}]*display:\s*none\s*!important;/s);
});

test("the Info privacy section links to a separate telemetry settings view", () => {
  const privacySection = source.match(/<section id="privacy"[\s\S]*?<\/section>/)?.[0] ?? "";
  const settingsView = source.match(/<div id="telemetry-settings"[\s\S]*?<div id="announcement-overlay"/)?.[0] ?? "";

  assert.match(privacySection, /id="telemetry-settings-link"/);
  assert.doesNotMatch(privacySection, /id="telemetry-toggle"/);
  assert.match(settingsView, /id="telemetry-toggle"/);
  assert.doesNotMatch(settingsView, /telemetry-(?:copy-id|reset)/);
});

test("iPad transient dialogs stay content-sized", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(
    styles,
    /\.platform-ipados #announcement-overlay\s*{[^}]*align-items:\s*center;/,
  );
});

test("iPhone uses one settings sheet and a dedicated device-family class", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(source, /appleUiPlatform === "ios"[\s\S]*?iphoneMenuActions\.append\([\s\S]*?iphonePerformanceSettings\.append\(controlDeck\)[\s\S]*?iphoneDisplaySettings\.append\(bottomControls\)/s);
  assert.match(source, /id="iphone-menu-overlay"[\s\S]*?role="dialog" aria-modal="true"/s);
  assert.match(styles, /\.platform-ios \.workspace[\s\S]*?grid-template-rows:\s*minmax\(0, 1fr\) calc\(\.5in \+ env\(safe-area-inset-bottom, 0px\)\);/s);
  assert.match(styles, /\.platform-ios \.tap-button\s*{[^}]*width:\s*100%;[^}]*height:\s*calc\(\.5in \+ env\(safe-area-inset-bottom, 0px\)\);[^}]*border-radius:\s*0;/s);
  assert.match(styles, /\.platform-ios \.performance-strip > :not\(\.tap-button\)\s*{[^}]*display:\s*none\s*!important;/s);
  assert.doesNotMatch(styles, /\.platform-ios \.empty-actions\s*{[^}]*display:\s*none;/s);
});

test("short iPhone landscape keeps empty-state actions visible and compacts only supporting content", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(source, /class="empty-landscape-extra"> \(like MuseScore[\s\S]*?class="empty-landscape-extra"> \(free options/);
  assert.match(source, /appleUiPlatform === "ios"\)\s*{\s*elements\.iphoneMenuActions\.append\(\s*elements\.open,\s*elements\.helpButton,/s);
  assert.match(source, /shell\?\.classList\.remove\("score-empty"\);[\s\S]*?appleUiPlatform === "ios"[\s\S]*?elements\.helpButton\.before\(elements\.demoChoirOpen, elements\.demoPianoOpen\);/s);
  assert.match(
    styles,
    /@media \(orientation: landscape\) and \(max-height: 500px\)\s*{[\s\S]*?\.platform-ios \.empty-landscape-extra\s*{[^}]*display:\s*none;[^}]*}[\s\S]*?\.platform-ios \.empty-devices\s*{[^}]*display:\s*none;/s,
  );
});

test("iPhone settings selects do not retain a focus highlight after choosing an option", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ios \.iphone-menu-overlay \.control-deck \.field select\s*{[^}]*-webkit-tap-highlight-color:\s*transparent;/s);
  assert.match(styles, /\.platform-ios \.iphone-menu-overlay \.control-deck \.field select:focus,[\s\S]*?select:focus-visible,[\s\S]*?select:active\s*{[^}]*outline:\s*none\s*!important;[^}]*box-shadow:\s*none;[^}]*background:\s*transparent;/s);
});

test("iPad header selects use light native menu arrows", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(
    styles,
    /\.platform-ipados \.control-deck \.field select\s*{[^}]*color-scheme:\s*dark;/s,
  );
});

test("Apple mobile performance UI suppresses browser gestures but leaves dialogs alone", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ipados \.shell > :not\(\[role="dialog"\]\)\s*{[^}]*-webkit-user-select:\s*none;[^}]*touch-action:\s*pan-x pan-y;/s);
  assert.match(source, /appleUiPlatform === "ipados" \|\| appleUiPlatform === "ios"[\s\S]*?"selectstart"[\s\S]*?"gesturestart"[\s\S]*?event\.touches\.length > 1/);
  assert.match(source, /target\.closest\('\[role="dialog"\]'\)/);
});

test("Apple mobile score gestures support bounded two-finger pan without pinch zoom", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ios \.score-scroll,[\s\S]*?\.platform-ipados \.score-scroll\s*{[^}]*touch-action:\s*pan-x pan-y;/s);
  assert.match(source, /function moveScoreTouchGesture[\s\S]*?scrollLeft = scoreTouchGesture\.startScrollLeft[\s\S]*?scrollTop = scoreTouchGesture\.startScrollTop/);
  assert.doesNotMatch(source, /startZoomPercent|previewZoomPercent|startDistance/);
  assert.match(source, /scoreScroll\.addEventListener\("touchstart"[\s\S]*?"touchmove"[\s\S]*?"touchend"[\s\S]*?"touchcancel"/);
});

test("iPad score actions use tighter platform-specific top spacing", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ipados \.slice-controls\s*{[^}]*top:\s*15px;[^}]*row-gap:\s*0;/s);
  assert.match(source, /AUDITION_PX_BELOW_HEADER = appleUiPlatform === "ipados" \? 15 : 20/);
  assert.match(source, /START_HERE_PX_BELOW_AUDITION_BOTTOM = appleUiPlatform === "ipados" \? 0 : 12/);
});

test("iPad orientation changes rebuild score and overlay geometry after layout settles", () => {
  assert.match(source, /ipadLandscapeQuery = window\.matchMedia\("\(orientation: landscape\)"\)/);
  assert.match(source, /refreshIpadOrientationLayout[\s\S]*?requestAnimationFrame[\s\S]*?requestAnimationFrame[\s\S]*?positionScoreActionRows\(\)[\s\S]*?restartIncrementalRendering\(currentTarget\)[\s\S]*?updateVisualPosition\(\)/);
  assert.match(source, /ipadLandscapeQuery\.addEventListener\("change", scheduleIpadOrientationLayoutRefresh\)/);
  assert.match(source, /window\.addEventListener\("orientationchange", scheduleIpadOrientationLayoutRefresh\)/);
  assert.match(source, /window\.addEventListener\("resize", scheduleIpadOrientationLayoutRefresh, \{ passive: true \}\)/);
});

test("iPad TAP button is wider and its transport buttons use adjacent columns", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ipados \.performance-strip\s*{[^}]*grid-template-columns:[^;]*420px/s);
  assert.match(styles, /\.platform-ipados \.tap-button\s*{[^}]*grid-column:\s*3;[^}]*width:\s*min\(420px, 100%\);/s);
  assert.match(styles, /\.platform-ipados #back-button\s*{[^}]*grid-column:\s*2;/s);
  assert.match(styles, /\.platform-ipados #forward-button\s*{[^}]*grid-column:\s*4;/s);
});

test("iPad landscape widens TAP by 60 percent and keeps circular transports symmetric", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /@media \(orientation: landscape\)[\s\S]*?\.platform-ipados \.performance-strip\s*{[^}]*grid-template-columns:\s*minmax\(0, 1fr\) 44px minmax\(150px, 672px\) 44px minmax\(0, 1fr\);[^}]*column-gap:\s*10px;/s);
  assert.match(styles, /@media \(orientation: landscape\)[\s\S]*?\.platform-ipados \.tap-button\s*{[^}]*width:\s*min\(672px, 100%\);/s);
  assert.doesNotMatch(styles, /@media \(max-width:\s*1100px\) and \(orientation:\s*landscape\)/);
  assert.match(styles, /\.transport\s*{[^}]*display:\s*grid;[^}]*place-items:\s*center;/s);
  assert.match(styles, /@media \(max-width: 1100px\)[\s\S]*?\.transport\s*{[^}]*width:\s*44px;[^}]*min-width:\s*44px;[^}]*height:\s*44px;[^}]*min-height:\s*44px;[^}]*aspect-ratio:\s*1;/s);
});

test("iPad landscape applies the compact footer at every device width", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  const landscapeStart = styles.indexOf("@media (orientation: landscape)");
  const landscapeEnd = styles.indexOf("\n}\n\n.platform-ipados .performance-strip", landscapeStart);
  const landscape = styles.slice(landscapeStart, landscapeEnd + 2);

  assert.match(landscape, /\.platform-ipados \.workspace\s*{[^}]*174px;/s);
  assert.match(landscape, /\.platform-ipados \.performance-strip\s*{[^}]*grid-template-rows:\s*minmax\(92px, 1fr\) 46px;/s);
  assert.match(landscape, /\.platform-ipados \.bottom-controls\s*{[^}]*grid-row:\s*2;[^}]*height:\s*46px;[^}]*display:\s*flex\s*!important;/s);
  assert.match(landscape, /\.platform-ipados \.bottom-controls input\[type="range"\]\s*{[^}]*height:\s*28px;[^}]*margin:\s*0;/s);
});

test("iPad collapsed chrome retains only the compact transport footer", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ipados \.shell\.chrome-hidden \.workspace\s*{[^}]*96px[^}]*safe-area-inset-bottom/s);
  assert.match(styles, /\.platform-ipados \.shell\.chrome-hidden \.performance-strip\s*{[^}]*display:\s*grid;[^}]*grid-template-rows:\s*92px;/s);
  assert.match(styles, /\.platform-ipados \.shell\.chrome-hidden \.bottom-controls\s*{[^}]*display:\s*none\s*!important;/s);
});

test("iPad collapsed chrome hides the entire footer while MIDI input is active", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(source, /function syncMidiInputChromeState\(\): void\s*{[^}]*classList\.toggle\("midi-input-active", selectedMidiInputId\.length > 0\);/s);
  assert.match(source, /selectedMidiInputId = requested;\s*syncMidiInputChromeState\(\);/s);
  assert.match(source, /selectedMidiInputId = inputSelection\.selectedId;[\s\S]*?elements\.midiInput\.value = selectedMidiInputId;\s*syncMidiInputChromeState\(\);/s);
  assert.match(source, /selectedMidiInputId = resolved\.id;\s*elements\.midiInput\.value = resolved\.id;\s*syncMidiInputChromeState\(\);/s);
  assert.match(styles, /\.platform-ipados \.shell\.chrome-hidden\.midi-input-active \.workspace\s*{[^}]*grid-template-rows:\s*minmax\(0, 1fr\) 0;/s);
  assert.match(styles, /\.platform-ipados \.shell\.chrome-hidden\.midi-input-active \.performance-strip\s*{[^}]*display:\s*none;/s);
});

test("iPad TAP copy is touch-specific and footer sliders use touch order", () => {
  assert.match(source, /class="ipad-tap-help">Hold for longer notes\. Use multiple fingers separately to hold each chord its desired length\./);
  assert.match(source, /appleUiPlatform === "ipados"\s*\? \[elements\.regularRoll\.parentElement, elements\.auditionRoll\.parentElement, elements\.volume\.parentElement, zoomControls\]/s);

  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(styles, /\.platform-ipados \.tap-button \.keyboard-tap-help\s*{[^}]*display:\s*none;/s);
  assert.match(styles, /\.platform-ipados \.tap-button \.ipad-tap-help\s*{[^}]*display:\s*block;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls \.range-field:nth-child\(2\)\s*{[^}]*border-right:\s*1px solid #3b3c34;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls \.zoom-controls\s*{[^}]*border-right:\s*0;/s);
});

test("Stop disables score tapping until conducting mode is restored", () => {
  assert.match(source, /elements\.tap\.disabled = midiFreePlay \|\| score === null;/);
  assert.match(source, /async function performDown[\s\S]*?if \(isPlaybackBlocked\(\) \|\| midiFreePlay\) return;/);
  assert.match(source, /elements\.tap\.disabled = midiFreePlay;/);
});

test("blocking waits cover the performance UI and suppress note input", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(source, /id="loading-overlay"[^>]+role="status"[^>]+aria-live="polite"/);
  assert.match(source, /const blockingWaits = new Map<BlockingWaitToken, string>\(\)/);
  assert.match(source, /const startupWait = beginBlockingWait\(t\("startingAudio"\)\)/);
  assert.match(source, /async function loadScore[\s\S]*?beginBlockingWait\(loadingMessage\)[\s\S]*?endBlockingWait\(wait\)/);
  assert.match(source, /async function reloadAudioSystems[\s\S]*?beginBlockingWait\(t\("reloadingDevices"\)\)[\s\S]*?endBlockingWait\(wait\)/);
  assert.match(source, /async function auditionDown[\s\S]*?if \(isPlaybackBlocked\(\)/);
  assert.match(source, /document\.addEventListener\("keydown"[\s\S]*?if \(isPlaybackBlocked\(\)\)/);
  assert.match(styles, /\.loading-overlay\s*{[^}]*position:\s*fixed;[^}]*inset:\s*0;[^}]*z-index:\s*500;/s);
  assert.match(styles, /\.loading-spinner\s*{[^}]*animation:\s*loading-spin/s);
});

test("compact footer controls leave vertical room for slider labels", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /@media \(max-width: 1100px\)[\s\S]*?\.performance-strip\s*{[^}]*grid-template-rows:\s*minmax\(92px, 1fr\) 46px;[^}]*padding:\s*4px [^;]* max\(4px, env\(safe-area-inset-bottom, 0px\)\)/s);
  assert.match(styles, /\.bottom-controls\s*{[^}]*height:\s*46px;/s);
  assert.match(styles, /\.bottom-controls \.range-field\s*{[^}]*gap:\s*0;[^}]*padding-block:\s*0;/s);
  assert.match(styles, /\.bottom-controls \.zoom-controls\s*{[^}]*row-gap:\s*0;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls \.range-field\s*{[^}]*gap:\s*4px;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls \.zoom-controls\s*{[^}]*row-gap:\s*4px;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls input\[type="range"\]\s*{[^}]*height:\s*28px;[^}]*min-height:\s*28px;[^}]*margin:\s*0;/s);
});

test("Apple header controls use platform-specific visual corrections", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-macos \.panic-button:not\(\.midi-free-play\)::before\s*{[^}]*transform:\s*translateY\(6px\);/s);
  assert.match(styles, /\.control-deck > \.select-field\s*{[^}]*padding-inline:\s*2\.5px;/s);
  assert.match(styles, /\.platform-ipados \.control-deck > \.field\s*{[^}]*justify-content:\s*flex-start;/s);
  assert.match(source, /appleUiPlatform === "ipados" \? "─{10}" : "─{12}"/);
});

test("collapsed header toggle stays left and header fields shrink by content", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.shell\.chrome-hidden \.topbar\s*{[^}]*justify-content:\s*flex-start;/s);
  assert.match(styles, /@media \(max-width: 1100px\)[\s\S]*?\.shell\.chrome-hidden \.topbar\s*{[^}]*padding-inline:\s*max\(12px, env\(safe-area-inset-left, 0px\)\)/s);
  assert.match(styles, /\.control-deck\s*{[^}]*overflow:\s*hidden;/s);
  assert.match(styles, /\.control-deck > \.select-field\s*{[^}]*flex-basis:\s*var\(--preferred-field-width, 92px\);[^}]*min-width:\s*0;/s);
  assert.match(styles, /\.control-deck > \.range-field\s*{[^}]*flex:\s*0 1 130px;/s);
  assert.match(source, /preferredSelectWidth = Math\.max\([\s\S]*?text\.length \* 7 \+ nativeControlAllowance,[\s\S]*?label\.length \* 7 \+ 8/s);
  assert.doesNotMatch(source, /Math\.min\(206, text\.length/);
});

test("score action rows have a fixed fallback and are positioned before engraving", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(styles, /\.slice-controls\s*{[^}]*top:\s*20px;[^}]*row-gap:\s*12px;/s);

  const positionCall = source.indexOf("positionScoreActionRows();");
  const engravingCall = source.indexOf("fitFirstSystemEngravingToActions(activeOsmd);");
  assert.ok(positionCall >= 0);
  assert.ok(engravingCall > positionCall);
});

test("desktop footer is 40 pixels shorter without shrinking its contents", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(styles, /\.workspace\s*{[^}]*grid-template-rows:\s*minmax\(0, 1fr\) 110px;/s);
  assert.match(styles, /\.performance-strip\s*{[^}]*padding:\s*5px [^;]* max\(5px, env\(safe-area-inset-bottom, 0px\)\)/s);
  assert.match(styles, /\.tap-button\s*{[^}]*height:\s*100px;/s);
});

test("the rhythm position highlight spans only the full score layer", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.score-scroll\s*{[^}]*overflow-x:\s*auto;[^}]*overflow-y:\s*auto;/s);
  assert.match(styles, /\.score-highlights\s*{[^}]*position:\s*absolute;[^}]*inset:\s*0;/s);
  assert.match(styles, /\.score-position-highlight\s*{[^}]*top:\s*0\s*!important;[^}]*height:\s*100%\s*!important;/s);
  assert.match(styles, /\.slice-controls\s*{[^}]*background:\s*transparent;/s);
  assert.match(styles, /\.slice-action\s*{[^}]*background:\s*transparent;/s);
});

test("normal position changes do not mutate OSMD or animate the engraving stack", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  const updateVisualPosition = source.match(
    /function updateVisualPosition\([^)]*\): void \{([\s\S]*?)\n\}/,
  )?.[1] ?? "";

  assert.doesNotMatch(updateVisualPosition, /osmd\.cursor|moveOsmdCursor/);
  assert.match(styles, /\.score-position-highlight\s*{[^}]*visibility:\s*hidden;/s);
  assert.match(styles, /\.score-position-highlight\.current\s*{[^}]*visibility:\s*visible;/s);
  assert.doesNotMatch(styles, /\.score-position-highlight\s*{[^}]*transition:/s);
});

test("ordinary buttons recover a click suppressed by native focus transitions", () => {
  assert.match(source, /const pressedButtons = new Map<number, HTMLButtonElement>\(\)/);
  assert.match(source, /document\.addEventListener\("pointerup"[\s\S]*?window\.setTimeout\([\s\S]*?pressed\.click\(\)/);
  assert.match(source, /document\.addEventListener\("click"[\s\S]*?window\.clearTimeout\(fallback\.timer\)/);
  assert.match(source, /start\.addEventListener\("click", reposition\)/);
  assert.doesNotMatch(source, /start\.addEventListener\("pointerdown", reposition\)/);
  assert.match(source, /data-pointer-activation="hold"/);
  assert.match(source, /button\.dataset\.pointerActivation = "hold"/);
});

test("pointer-managed Audition and TAP buttons recover click-only activation without double-playing", () => {
  assert.match(source, /const pointerManagedButtonPresses = new WeakMap<HTMLButtonElement, PointerManagedButtonPresses>\(\)/);
  assert.match(source, /function shouldActivatePointerManagedButtonFromClick[\s\S]*?!event\.isTrusted[\s\S]*?pointerManagedButtonPresses\.get\(button\)[\s\S]*?return false/);
  assert.match(source, /app\.addEventListener\("click"[\s\S]*?shouldActivatePointerManagedButtonFromClick\(match\.button, event\)/);
  assert.match(source, /elements\.tap\.addEventListener\("click"[\s\S]*?shouldActivatePointerManagedButtonFromClick\(elements\.tap, event\)/);
  assert.doesNotMatch(source, /app\.addEventListener\("click", \(event\) => \{\s*if \(event\.detail !== 0\) return/);
  assert.doesNotMatch(source, /elements\.tap\.addEventListener\("click", \(event\) => \{\s*if \(event\.detail !== 0\) return/);
});

test("Audition and TAP recover immediate mouse-down holds when pointerdown is omitted", () => {
  assert.match(source, /app\.addEventListener\("mousedown"[\s\S]*?isPointerManagedButtonPressed\(match\.button\)[\s\S]*?auditionDown\(token, index, match\.target\.midiPitches\)[\s\S]*?createPointerHold/);
  assert.match(source, /elements\.tap\.addEventListener\("mousedown"[\s\S]*?isPointerManagedButtonPressed\(elements\.tap\)[\s\S]*?performDown\(token\)[\s\S]*?createPointerHold/);
  assert.match(source, /window\.addEventListener\("mouseup"[\s\S]*?finishPointerManagedButtonPress\(button, -1\)[\s\S]*?releasePointerHold\(hold\)/);
  assert.match(source, /function beginPointerManagedButtonPress[\s\S]*?pointerIds\.add\(pointerId\)/);
});

test("delayed iOS compatibility clicks are deduplicated after touch release", () => {
  assert.match(source, /const CLICK_DEDUPLICATION_MS = 2_000/);
  assert.match(source, /finishPointerManagedButtonPress\(pointerManaged, event\.pointerId\)/);
  assert.match(source, /function consumeCompletedButtonClick[\s\S]*?!event\.isTrusted[\s\S]*?completed\.pointerId !== eventPointerId[\s\S]*?clearCompletedButtonClick/);
  assert.match(source, /markCompletedButtonClick\(recoveredOrdinaryButtonClicks, pressed, fallback\.pointerId\)/);
  assert.match(source, /consumeCompletedButtonClick\(recoveredOrdinaryButtonClicks, button, event\)[\s\S]*?event\.stopImmediatePropagation\(\)/);
});

test("iOS pointer retargeting and lost capture still complete held-button clicks", () => {
  const pointerUpRecovery = source.match(/document\.addEventListener\("pointerup"[\s\S]*?\}, \{ capture: true \}\);/)?.[0] ?? "";
  assert.match(pointerUpRecovery, /if \(pointerManaged\)[\s\S]*?finishPointerManagedButtonPress\(pointerManaged, event\.pointerId\)/);
  assert.doesNotMatch(pointerUpRecovery, /enabledButtonFromPointer\(event\) === pointerManaged/);
  assert.match(source, /document\.addEventListener\("lostpointercapture"[\s\S]*?finishPointerManagedButtonPress\(pointerManaged, event\.pointerId\)/);
});

test("iOS compatibility mouse events cannot start a second hold after touch release", () => {
  assert.match(source, /const COMPATIBILITY_MOUSE_BLOCK_MS = 1_500/);
  assert.match(source, /function noteNonMousePointerEvent[\s\S]*?event\.pointerType !== "mouse"[\s\S]*?compatibilityMouseBlockedUntil/);
  assert.match(source, /function isTouchCompatibilityMouseEvent[\s\S]*?appleUiPlatform === "ios"[\s\S]*?firesTouchEvents[\s\S]*?compatibilityMouseBlockedUntil/);
  assert.match(source, /app\.addEventListener\("mousedown"[\s\S]*?isTouchCompatibilityMouseEvent\(event\)/);
  assert.match(source, /elements\.tap\.addEventListener\("mousedown"[\s\S]*?isTouchCompatibilityMouseEvent\(event\)/);
  assert.match(source, /document\.addEventListener\("pointerup"[\s\S]*?noteNonMousePointerEvent\(event\)/);
});
