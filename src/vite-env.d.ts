// Copyright (c) 2026 Michael Saunders
/// <reference types="vite/client" />

declare const __TAPCONDUCTOR_VERSION__: string;

interface ImportMetaEnv {
  readonly VITE_POSTHOG_PROJECT_KEY?: string;
  readonly VITE_POSTHOG_HOST?: string;
  readonly VITE_BUILD_NUMBER?: string;
  readonly VITE_RELEASE_CHANNEL?: string;
}
