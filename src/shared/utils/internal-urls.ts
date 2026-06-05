export const BROWSER_HOME_URL = 'browser://home';
export const BROWSER_SETTINGS_URL = 'browser://settings';

export type InternalUrl = typeof BROWSER_HOME_URL | typeof BROWSER_SETTINGS_URL;

export function isInternalBrowserUrl(url: string): url is InternalUrl {
  return url === BROWSER_HOME_URL || url === BROWSER_SETTINGS_URL;
}

export function titleForInternalUrl(url: InternalUrl): string {
  if (url === BROWSER_SETTINGS_URL) {
    return 'Settings';
  }

  return 'New Tab';
}
