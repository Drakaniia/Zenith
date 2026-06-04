import { getSettings, saveSettings, type AppSettings } from '../utils/tauri';
import { toastsState } from './toasts.svelte.js';

export const DEFAULT_SETTINGS: AppSettings = {
  default_search_engine: 'yandex',
  homepage: 'https://yandex.com',
  theme: 'dark',
  javascript_enabled: true,
  show_status_bar: true
};

class SettingsState {
  settings = $state<AppSettings>({ ...DEFAULT_SETTINGS });
  isLoading = $state(false);
  isSaving = $state(false);
  isDarkTheme = $derived(this.settings.theme === 'dark');

  load = async (): Promise<void> => {
    this.isLoading = true;

    try {
      this.settings = await getSettings();
    } catch (error) {
      toastsState.error(`Failed to load settings: ${errorMessage(error)}`);
    } finally {
      this.isLoading = false;
    }
  };

  save = async (settings: AppSettings = this.settings): Promise<void> => {
    this.isSaving = true;

    try {
      const snapshot = $state.snapshot(settings);
      await saveSettings(snapshot);
      this.settings = { ...snapshot };
    } catch (error) {
      toastsState.error(`Failed to save settings: ${errorMessage(error)}`);
    } finally {
      this.isSaving = false;
    }
  };

  update = (patch: Partial<AppSettings>): void => {
    this.settings = {
      ...this.settings,
      ...patch
    };
  };

  resetLocal = (): void => {
    this.settings = { ...DEFAULT_SETTINGS };
  };
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  return 'Unknown error';
}

export const settingsState = new SettingsState();
