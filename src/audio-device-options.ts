// Copyright (c) 2026 Michael Saunders
import type { DeviceDto } from "./types";

export interface AudioDeviceOption {
  label: string;
  value: string;
}

/**
 * Build the selectable native audio routes. The endpoint marked as default is
 * the concrete device currently reached by the system-default route, so
 * presenting both would offer the same output twice. Keep its useful device
 * name, but use the empty value that tells the native backend to follow the
 * system default when the OS route changes.
 */
export function audioDeviceOptions(devices: readonly DeviceDto[]): AudioDeviceOption[] {
  const defaultDevice = devices.find((device) => device.isDefault);
  const options: AudioDeviceOption[] = [{
    label: defaultDevice ? `${defaultDevice.name} (default)` : "System default",
    value: "",
  }];

  for (const device of devices) {
    if (device.isDefault) continue;
    options.push({ label: device.name, value: device.id });
  }
  return options;
}
