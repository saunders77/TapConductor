// Copyright (c) 2026 Michael Saunders
export type AppleUiPlatform = "ipados" | "macos" | null;
export type InitialTelemetryAction = "enable" | "disable" | "prompt";

export function detectAppleUiPlatform(
  userAgent: string = navigator.userAgent,
  maxTouchPoints: number = navigator.maxTouchPoints,
): AppleUiPlatform {
  // Since iPadOS 13, Safari and WKWebView can identify an iPad as a Mac.
  // Multiple touch points distinguish that desktop-class user agent from macOS.
  if (/iPad/i.test(userAgent) || (/Macintosh/i.test(userAgent) && maxTouchPoints > 1)) {
    return "ipados";
  }
  return /Macintosh/i.test(userAgent) ? "macos" : null;
}

/**
 * Windows can persist a choice from its NSIS finish page. Apple distributions
 * do not have a customizable install step, so a fresh macOS/iPadOS install
 * makes the equivalent choice in the app before telemetry starts.
 */
export function initialTelemetryAction(
  installerConsent: boolean | null,
  webBuild: boolean,
  applePlatform: AppleUiPlatform,
): InitialTelemetryAction {
  if (installerConsent === true) return "enable";
  if (installerConsent === false) return "disable";
  if (!webBuild && applePlatform !== null) return "prompt";
  return "disable";
}
