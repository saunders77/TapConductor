// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { AsyncSerialQueue } from "./async-serial-queue.ts";

test("serial queue preserves arrival order across asynchronous work", async () => {
  const queue = new AsyncSerialQueue();
  const order: string[] = [];
  let releaseFirst: (() => void) | undefined;
  const firstGate = new Promise<void>((resolve) => {
    releaseFirst = resolve;
  });

  const first = queue.run(async () => {
    order.push("first-start");
    await firstGate;
    order.push("first-end");
  });
  const second = queue.run(async () => {
    order.push("second");
  });

  await Promise.resolve();
  assert.deepEqual(order, ["first-start"]);
  releaseFirst!();
  await Promise.all([first, second]);
  assert.deepEqual(order, ["first-start", "first-end", "second"]);
});

test("serial queue continues after a rejected task", async () => {
  const queue = new AsyncSerialQueue();
  await assert.rejects(queue.run(async () => {
    throw new Error("expected");
  }));
  assert.equal(await queue.run(async () => 42), 42);
});
