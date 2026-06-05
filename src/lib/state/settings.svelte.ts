import { getSettings, saveSettings, type AppSettings } from '../utils/tauri';
import { toastsState } from './toasts.svelte.js';

export type SearchEngine = 'yandex' | 'google';
export type ThemePreference = 'dark' | 'light';

export type SettingsPatch = Partial<
  Omit<AppSettings, 'default_search_engine' | 'theme'> & {
    default_search_engine: SearchEngine;
    theme: ThemePreference;
  }
>;

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
  lastSavedAt = $state<string | null>(null);
  isDarkTheme = $derived(this.settings.theme === 'dark');

  load = async (): Promise<void> => {
    this.isLoading = true;

    try {
      this.settings = normalizeSettings(await getSettings());
    } catch (error) {
      toastsState.error(`Failed to load settings: ${errorMessage(error)}`);
    } finally {
      this.isLoading = false;
    }
  };

  save = async (settings: AppSettings = this.settings): Promise<boolean> => {
    this.isSaving = true;

    try {
      const snapshot = normalizeSettings($state.snapshot(settings));
      await saveSettings(snapshot);
      this.settings = { ...snapshot };
      this.lastSavedAt = new Date().toISOString();
      return true;
    } catch (error) {
      toastsState.error(`Failed to save settings: ${errorMessage(error)}`);
      return false;
    } finally {
      this.isSaving = false;
    }
  };

  update = (patch: SettingsPatch): void => {
    this.settings = normalizeSettings({
      ...this.settings,
      ...patch
    });
  };

  resetLocal = (): void => {
    this.settings = { ...DEFAULT_SETTINGS };
  };
}

function normalizeSettings(settings: AppSettings): AppSettings {
  return {
    ...settings,
    default_search_engine: isSearchEngine(settings.default_search_engine)
      ? settings.default_search_engine
      : DEFAULT_SETTINGS.default_search_engine,
    theme: isThemePreference(settings.theme) ? settings.theme : DEFAULT_SETTINGS.theme
  };
}

function isSearchEngine(value: string): value is SearchEngine {
  return value === 'yandex' || value === 'google';
}

function isThemePreference(value: string): value is ThemePreference {
  return value === 'dark' || value === 'light';
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
