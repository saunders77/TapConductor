// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import {
  announcementContentIdentifier,
  announcementIdentifier,
  shouldShowAnnouncement,
} from "./announcement.ts";

test("an empty announcement is never shown", () => {
  assert.equal(shouldShowAnnouncement(" \n", "2026-08-06T01:00:00Z", null), false);
});

test("a permanently dismissed announcement stays hidden", () => {
  const timestamp = "2026-08-06T01:00:00Z";
  assert.equal(shouldShowAnnouncement("Hello", timestamp, timestamp), false);
});

test("a later announcement is shown even after the prior one was dismissed", () => {
  assert.equal(
    shouldShowAnnouncement("New announcement", "2026-08-07T01:00:00Z", "2026-08-06T01:00:00Z"),
    true,
  );
});

test("the commit timestamp is preferred and content is a stable fallback", () => {
  assert.equal(
    announcementIdentifier("Hello", "2026-08-06T01:00:00Z", '"etag"', "yesterday"),
    "2026-08-06T01:00:00Z",
  );
  assert.equal(announcementIdentifier("Hello"), announcementContentIdentifier("Hello"));
  assert.notEqual(announcementContentIdentifier("Hello"), announcementContentIdentifier("Hello!"));
});

