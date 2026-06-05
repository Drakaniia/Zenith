import { getSettings, saveSettings, type AppSettings } from '../utils/tauri';
import { BROWSER_HOME_URL } from '../utils/internalUrls';
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
  homepage: BROWSER_HOME_URL,
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
    homepage: resolveHomepageUrl(settings.homepage),
    theme: isThemePreference(settings.theme) ? settings.theme : DEFAULT_SETTINGS.theme
  };
}

export function resolveHomepageUrl(homepage: string): string {
  const trimmed = homepage.trim();

  if (!trimmed || trimmed === BROWSER_HOME_URL) {
    return BROWSER_HOME_URL;
  }

  try {
    const url = new URL(trimmed);

    if (isLegacyYandexHomepage(url)) {
      return BROWSER_HOME_URL;
    }

    if (url.protocol === 'http:' || url.protocol === 'https:') {
      return trimmed;
    }
  } catch {
    return BROWSER_HOME_URL;
  }

  return BROWSER_HOME_URL;
}

function isLegacyYandexHomepage(url: URL): boolean {
  return (
    url.protocol === 'https:' &&
    url.hostname.toLocaleLowerCase() === 'yandex.com' &&
    (url.pathname === '' || url.pathname === '/') &&
    url.search === '' &&
    url.hash === ''
  );
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
