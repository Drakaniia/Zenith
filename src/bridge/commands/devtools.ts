import { invoke } from '@tauri-apps/api/core';
import { invokeCommand } from '../invoke';
import { isElectronRuntime } from '../runtime';

export function toggleNativeDevtools(): Promise<void> {
  if (isElectronRuntime()) {
    return invokeCommand<void>('toggle_devtools');
  }

  return invoke<void>('plugin:devtools-app|toggle');
}

export const toggleCrabNebulaDevtools = toggleNativeDevtools;
