// Copyright (c) 2026 Michael Saunders
export type AppleUiPlatform = "ios" | "ipados" | "macos" | null;
export type InitialTelemetryAction = "enable" | "disable";

export function detectAppleUiPlatform(
  userAgent: string = navigator.userAgent,
  maxTouchPoints: number = navigator.maxTouchPoints,
): AppleUiPlatform {
  // Use the device family reported by WebKit rather than viewport or pixel
  // dimensions. Modern iPhones have enough pixels to overlap tablet and
  // desktop breakpoints, so dimensions are not a reliable device signal.
  if (/iPhone|iPod/i.test(userAgent)) return "ios";
  // Since iPadOS 13, Safari and WKWebView can identify an iPad as a Mac.
  // Multiple touch points distinguish that desktop-class user agent from macOS.
  if (/iPad/i.test(userAgent) || (/Macintosh/i.test(userAgent) && maxTouchPoints > 1)) {
    return "ipados";
  }
  return /Macintosh/i.test(userAgent) ? "macos" : null;
}

/** Apply an explicit Windows-installer opt-out; otherwise telemetry defaults on. */
export function initialTelemetryAction(installerConsent: boolean | null): InitialTelemetryAction {
  if (installerConsent === false) return "disable";
  return "enable";
}
