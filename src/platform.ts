// Copyright (c) 2026 Michael Saunders
import type { BeatMidiInput, CoreEvent, DiagnosticsDto } from "./types";
import { WebRuntime } from "./web-runtime";

export type UnlistenFn = () => void;
type EventHandler<T> = (event: { payload: T }) => void;

const isTauri = "__TAURI_INTERNALS__" in window;
const webRuntime = isTauri ? null : new WebRuntime();

export function isWebBuild(): boolean {
  return !isTauri;
}

export async function openExternalUrl(url: string): Promise<void> {
  if (webRuntime) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }

  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

export function setAppWindowTitle(fileName?: string): void {
  const title = fileName ? `TapConductor: ${fileName}` : "TapConductor";
  document.title = title;
  if (webRuntime) return;

  void import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
    .catch((error: unknown) => {
      console.warn("The native window title could not be updated.", error);
    });
}

export async function appInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (webRuntime) return webRuntime.invoke<T>(command, args);
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export async function appListen<T>(
  event: string,
  handler: EventHandler<T>,
): Promise<UnlistenFn> {
  if (webRuntime) return webRuntime.listen(event, handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, handler);
}

export async function openScoreDialog(): Promise<File | string | null> {
  if (!webRuntime) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    return open({
      multiple: false,
      directory: false,
      pickerMode: "document",
      fileAccessMode: "copy",
      filters: [{ name: "Musical scores", extensions: ["musicxml", "xml", "mxl", "mid", "midi"] }],
    });
  }

  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".musicxml,.xml,.mxl,.mid,.midi";
    input.addEventListener("change", () => resolve(input.files?.[0] ?? null), { once: true });
    input.addEventListener("cancel", () => resolve(null), { once: true });
    input.click();
  });
}

// These imports keep event payloads checked when the browser implementation
// emits native-compatible events.
export type PlatformCoreEvent = CoreEvent;
export type PlatformDiagnostics = DiagnosticsDto;
export type PlatformBeatMidiInput = BeatMidiInput;
