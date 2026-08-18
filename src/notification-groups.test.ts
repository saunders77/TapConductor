// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { recordNotificationOccurrence, type NotificationOccurrences } from "./notification-groups.ts";

test("notifications accumulate by stable type even when their messages differ", () => {
  const group: NotificationOccurrences = { count: 0, messages: [] };

  recordNotificationOccurrence(group, "Actual extent 5 exceeds 4");
  recordNotificationOccurrence(group, "Actual extent 7 exceeds 6");

  assert.equal(group.count, 2);
  assert.deepEqual(group.messages, ["Actual extent 5 exceeds 4", "Actual extent 7 exceeds 6"]);
});

test("different notification types remain separate regardless of severity", () => {
  const history = new Map<string, NotificationOccurrences>();
  history.set("performance.ended", recordNotificationOccurrence({ count: 0, messages: [] }, "End of score"));
  history.set("audio.lifecycle", recordNotificationOccurrence({ count: 0, messages: [] }, "Audio stopped"));

  assert.equal(history.get("performance.ended")?.count, 1);
  assert.equal(history.get("audio.lifecycle")?.count, 1);
});
