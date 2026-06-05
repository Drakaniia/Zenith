const INTERNAL_PROTOCOL = 'browser:';
const FALLBACK_FILENAME = 'download';

export function isInternalUrl(url: string): boolean {
  return parseUrl(url)?.protocol === INTERNAL_PROTOCOL;
}

export function isSecureUrl(url: string): boolean {
  return parseUrl(url)?.protocol === 'https:';
}

export function displayUrl(url: string): string {
  const trimmed = url.trim();
  if (!trimmed || isInternalUrl(trimmed)) {
    return trimmed;
  }

  const parsed = parseUrl(trimmed);
  if (!parsed || !isWebUrl(parsed)) {
    return trimmed;
  }

  const path = `${parsed.pathname}${parsed.search}${parsed.hash}`;
  return `${parsed.host}${path === '/' ? '' : path}`;
}

export function domainFromUrl(url: string): string {
  const parsed = parseUrl(url);
  return parsed?.hostname ?? '';
}

export function suggestFilenameFromUrl(url: string): string {
  const parsed = parseUrl(url);
  if (!parsed || !isWebUrl(parsed)) {
    return FALLBACK_FILENAME;
  }

  const segment = parsed.pathname.split('/').filter(Boolean).at(-1) ?? parsed.hostname;
  return sanitizeFilename(decodeUrlComponent(segment));
}

function parseUrl(url: string): URL | null {
  try {
    return new URL(url.trim());
  } catch {
    return null;
  }
}

function isWebUrl(url: URL): boolean {
  return url.protocol === 'http:' || url.protocol === 'https:';
}

function decodeUrlComponent(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function sanitizeFilename(value: string): string {
  const sanitized = value
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_')
    .replace(/\s+/g, ' ')
    .trim();
  const withoutOuterDots = sanitized.replace(/^\.+/, '').replace(/\.+$/, '');

  return withoutOuterDots || FALLBACK_FILENAME;
}
