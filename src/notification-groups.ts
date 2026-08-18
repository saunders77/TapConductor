// Copyright (c) 2026 Michael Saunders
export interface NotificationOccurrences {
  count: number;
  messages: string[];
}

export function recordNotificationOccurrence(
  occurrences: NotificationOccurrences,
  message: string,
): NotificationOccurrences {
  occurrences.count += 1;
  occurrences.messages.push(message);
  return occurrences;
}
