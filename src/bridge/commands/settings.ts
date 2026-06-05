import type { AppSettings } from '../../shared/types/backend';
import { invokeCommand } from '../invoke';

export function getSettings(): Promise<AppSettings> {
  return invokeCommand<AppSettings>('get_settings');
}

export function saveSettings(settings: AppSettings): Promise<void> {
  return invokeCommand<void>('save_settings', { settings });
}
