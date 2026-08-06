// Copyright (c) 2026 Michael Saunders

export function announcementContentIdentifier(content: string): string {
  let hash = 2_166_136_261;
  for (let index = 0; index < content.length; index += 1) {
    hash ^= content.charCodeAt(index);
    hash = Math.imul(hash, 16_777_619);
  }
  return `content-${(hash >>> 0).toString(16)}`;
}

export function announcementIdentifier(
  content: string,
  updatedAt?: string,
  etag?: string | null,
  lastModified?: string | null,
): string {
  return updatedAt || etag || lastModified || announcementContentIdentifier(content);
}

export function shouldShowAnnouncement(content: string, identifier: string, dismissed: string | null): boolean {
  return content.trim().length > 0 && identifier !== dismissed;
}

