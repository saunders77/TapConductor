# Startup announcements

TapConductor reads `LATEST_ANNOUNCEMENT.md` from the repository's public `main` branch at startup.
Leave that file empty to show nothing. Commit new content to publish an announcement.

The dialog supports paragraphs, lists, small headings, `<strong>`/`**bold**`, `<em>`, and HTTPS or
HTTP links written as `<a href="https://example.com">text</a>` or `[text](https://example.com)`.
Other markup and attributes are removed before display, and links open in the system browser.

When a user selects **Don't show this announcement again**, the app stores the announcement file's
latest GitHub commit timestamp locally. Editing and committing the file produces a new timestamp, so
the next announcement can still appear. If GitHub's commit endpoint is unavailable, the app falls
back to the file's ETag, Last-Modified value, or a stable content identifier.

