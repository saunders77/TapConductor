// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const nativeSource = readFileSync(
  new URL("../src-tauri/src/lib.rs", import.meta.url),
  "utf8",
);
const audioRuntimeSource = readFileSync(
  new URL("../src-tauri/src/audio_runtime.rs", import.meta.url),
  "utf8",
);
const midiBackendSource = readFileSync(
  new URL("../crates/tapconductor-midi/src/backend.rs", import.meta.url),
  "utf8",
);

test("a successful audio reconnection dismisses the inactive-audio error", () => {
  assert.match(
    source,
    /if \(diagnostics\.ready\) dismissInactiveAudioError\(\)/,
  );
  assert.match(source, /const displayedErrorToasts = new Map<string, HTMLElement>\(\)/);
  assert.match(source, /displayedErrorToasts\.set\(message, item\)/);
  assert.match(source, /listen<void>\("audio-lifecycle-restored"[\s\S]*?refreshDiagnostics\(\)/);
});

test("audio recovery instructions expose Reload as a direct action", () => {
  assert.match(
    source,
    /function appendAudioReloadPrompt[\s\S]*?reload\.addEventListener\("click"[\s\S]*?await reloadAudioSystems\(\)/,
  );
  assert.match(
    source,
    /toast\(message, "error", true, "audio\.output-not-ready"\)/,
  );
  assert.match(
    source,
    /key === t\("state"\) && !diagnostics\.ready[\s\S]*?appendAudioReloadPrompt/,
  );
});

test("iOS restores suspended audio whenever its scene becomes active", () => {
  assert.match(
    nativeSource,
    /WindowEvent::Resumed \| tauri::WindowEvent::Focused\(true\)/,
  );
  assert.match(nativeSource, /if !core\.audio\.is_suspended\(\)/);
  assert.match(
    nativeSource,
    /fn restore_mobile_audio[\s\S]*?emit\("audio-lifecycle-restoring", \(\)\)[\s\S]*?core\.resume_audio\(\)/,
  );
  assert.match(nativeSource, /emit\("audio-lifecycle-restored", \(\)\)/);
  assert.match(
    source,
    /listen<void>\("audio-lifecycle-restoring"[\s\S]*?beginBlockingWait\(t\("restoringAudio"\)\)/,
  );
  assert.match(
    source,
    /listen<void>\("audio-lifecycle-restored"[\s\S]*?endBlockingWait\(audioRecoveryWait\)/,
  );
  assert.match(audioRuntimeSource, /pub const fn is_suspended\(&self\) -> bool/);
  assert.match(audioRuntimeSource, /self\.suspended = false/);
});

test("iOS WebKit foreground events provide an idempotent audio recovery fallback", () => {
  assert.match(
    source,
    /function restoreAudioAfterForeground[\s\S]*?appleUiPlatform !== "ios"[\s\S]*?appleUiPlatform !== "ipados"[\s\S]*?invoke<void>\("restore_audio_after_foreground"\)/,
  );
  assert.match(
    source,
    /visibilitychange[\s\S]*?if \(!document\.hidden\) restoreAudioAfterForeground\(\)/,
  );
  assert.match(source, /window\.addEventListener\("focus", restoreAudioAfterForeground\)/);
  assert.match(nativeSource, /restore_mobile_audio\(_window\.app_handle\(\), state\.inner\(\)\)/);
  assert.match(audioRuntimeSource, /Err\(resume_error\) => self\.reload\(\)/);
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

test("CoreMIDI setup changes refresh MIDI devices on macOS and iOS", () => {
  assert.match(source, /listen<void>\("midi-devices-changed", scheduleMidiDeviceRefresh\)/);
  assert.match(
    source,
    /function scheduleMidiDeviceRefresh\(\)[\s\S]*?appleUiPlatform !== "ipados"[\s\S]*?window\.setTimeout[\s\S]*?void refreshDevices\(\)/,
  );
  assert.match(
    source,
    /async function refreshDevices\(\)[\s\S]*?await syncMacosMenu\(\);\s*\}/,
  );
  assert.match(
    nativeSource,
    /cfg\(any\(target_os = "macos", target_os = "ios"\)\)[\s\S]*?Client::new_with_notifications/,
  );
  assert.match(
    nativeSource,
    /WindowEvent::Resumed \| tauri::WindowEvent::Focused\(true\)[\s\S]*?emit\("midi-devices-changed", \(\)\)/,
  );
  assert.match(
    midiBackendSource,
    /cfg\(any\(target_os = "macos", target_os = "ios"\)\)\]\s*coremidi::restart\(\)/,
  );
  assert.match(
    midiBackendSource,
    /cfg\(target_os = "ios"\)[\s\S]*?coremidi::Sources[\s\S]*?coremidi::Destinations/,
  );
  assert.match(source, /MIDI inputs detected[\s\S]*?midiInputsAvailable/);
  assert.match(source, /MIDI input discovery error[\s\S]*?midiInputDiscoveryError/);
});

test("first-run device failures stay out of notifications and remain visible in diagnostics", () => {
  assert.match(
    source,
    /function toast\([\s\S]*?if \(isFirstRunScreen\(\) && isDeviceSetupNotification\(type\)\) return;[\s\S]*?notificationHistory\.get/,
  );
  assert.match(
    source,
    /function isDeviceSetupNotification[\s\S]*?type\.startsWith\("audio\."\)[\s\S]*?type\.startsWith\("midi\."\)/,
  );
  assert.match(
    source,
    /for \(const error of firstRunDeviceSetupErrors\)[\s\S]*?rows\.push\(\[error\.label, error\.message\]\)/,
  );
});

test("a first-run MIDI failure disables MIDI and restores the system audio route", () => {
  assert.match(
    source,
    /async function resetFirstRunMidiAndAudioToDefaults[\s\S]*?invoke\("set_midi_input", \{ id: null \}\)[\s\S]*?invoke\("set_midi_output", \{ id: null \}\)[\s\S]*?invoke\("set_audio_device", \{ id: "" \}\)/,
  );
  assert.match(
    source,
    /delete persistedSettings\.midiInput;[\s\S]*?delete persistedSettings\.midiOutput;[\s\S]*?delete persistedSettings\.audioOutput;/,
  );
  assert.match(
    source,
    /if \(midiSetupFailed && isFirstRunScreen\(\)\) \{\s*await resetFirstRunMidiAndAudioToDefaults\(\);/,
  );
});

test("native audio remembers launch volume even before an endpoint is ready", () => {
  assert.match(
    audioRuntimeSource,
    /pub fn set_volume[\s\S]*?self\.master_gain = gain;[\s\S]*?\.runtime[\s\S]*?ok_or_else\(\|\| "Audio is not ready\."/,
  );
});
