export type AppleUiPlatform = "ipados" | "macos" | null;

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
