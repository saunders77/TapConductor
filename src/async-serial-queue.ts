// Copyright (c) 2026 Michael Saunders

/** Runs short asynchronous state transitions in arrival order. A rejected
 * transition does not poison the queue for later work. */
export class AsyncSerialQueue {
  private tail: Promise<void> = Promise.resolve();

  run<T>(task: () => Promise<T>): Promise<T> {
    const result = this.tail.then(task);
    this.tail = result.then(() => undefined, () => undefined);
    return result;
  }
}
