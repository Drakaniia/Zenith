export const DEFAULT_SETTINGS = {
  default_search_engine: 'yandex',
  homepage: 'browser://home',
  theme: 'dark',
  javascript_enabled: true,
  show_status_bar: true
};

const METADATA_TIMEOUT_MS = 10000;
const METADATA_USER_AGENT = 'Ampui/0.1 metadata fetcher';

export function normalizeUrlWithSettings(input, settings = DEFAULT_SETTINGS) {
  const trimmed = String(input ?? '').trim();

  if (!trimmed) {
    throw new Error('URL or search query cannot be empty');
  }

  if (isLoopbackHostWithoutScheme(trimmed)) {
    return validateWebUrl(`http://${trimmed}`);
  }

  if (looksLikeAbsoluteUrl(trimmed)) {
    return validateWebUrl(trimmed);
  }

  if (looksLikeDomain(trimmed)) {
    return validateWebUrl(`https://${trimmed}`);
  }

  return searchUrl(trimmed, settings);
}

export async function fetchPageMetadata(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), METADATA_TIMEOUT_MS);

  try {
    const response = await fetch(url, {
      headers: {
        'user-agent': METADATA_USER_AGENT
      },
      signal: controller.signal
    });

    if (!response.ok) {
      throw new Error(`failed to fetch page metadata: ${response.status} ${response.statusText}`);
    }

    const html = await response.text();
    return pageMetadataFromHtml(html, response.url, url);
  } catch (error) {
    if (error?.name === 'AbortError') {
      throw new Error('failed to fetch page metadata: request timed out');
    }

    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

export function pageMetadataFromHtml(html, pageUrl, fallbackTitle) {
  const title = extractTitle(html);
  const faviconUrl = title ? extractFaviconUrl(html, pageUrl) : null;

  return {
    title: title || fallbackTitle,
    favicon_url: faviconUrl
  };
}

function isLoopbackHostWithoutScheme(input) {
  const lower = input.toLocaleLowerCase();
  const localNames = ['localhost', '127.0.0.1', '[::1]'];

  return localNames.some(
    (name) =>
      lower === name ||
      lower.startsWith(`${name}:`) ||
      lower.startsWith(`${name}/`) ||
      lower.startsWith(`${name}?`) ||
      lower.startsWith(`${name}#`)
  );
}

function looksLikeAbsoluteUrl(input) {
  try {
    new URL(input);
    return true;
  } catch {
    return false;
  }
}

function looksLikeDomain(input) {
  if (/\s/.test(input)) {
    return false;
  }

  const hostPart = input.split(/[/?#]/, 1)[0].replace(/\.+$/, '');
  return hostPart.includes('.');
}

function validateWebUrl(input) {
  let parsed;

  try {
    parsed = new URL(input);
  } catch (error) {
    throw new Error(`invalid URL: ${error.message}`);
  }

  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`unsupported URL scheme: ${parsed.protocol.replace(/:$/, '')}`);
  }

  if (!parsed.hostname) {
    throw new Error('URL must include a host');
  }

  return input;
}

function searchUrl(query, settings) {
  const encodedQuery = encodeURIComponent(query);

  if (settings?.default_search_engine?.toLocaleLowerCase() === 'google') {
    return `https://www.google.com/search?q=${encodedQuery}`;
  }

  return `https://yandex.com/search/?text=${encodedQuery}`;
}

function extractTitle(html) {
  const match = /<title[^>]*>([\s\S]*?)<\/title>/i.exec(html);
  return decodeHtmlText(match?.[1] ?? '').trim();
}

function extractFaviconUrl(html, pageUrl) {
  const linkPattern = /<link\b[^>]*>/gi;
  let match;

  while ((match = linkPattern.exec(html)) !== null) {
    const tag = match[0];
    const rel = attributeValue(tag, 'rel')?.toLocaleLowerCase();

    if (!rel?.split(/\s+/).includes('icon')) {
      continue;
    }

    const href = attributeValue(tag, 'href');
    if (!href) {
      continue;
    }

    try {
      return new URL(href, pageUrl).toString();
    } catch {
      return null;
    }
  }

  return null;
}

function attributeValue(tag, name) {
  const pattern = new RegExp(`\\b${name}\\s*=\\s*("([^"]*)"|'([^']*)'|([^\\s>]+))`, 'i');
  const match = pattern.exec(tag);
  return match?.[2] ?? match?.[3] ?? match?.[4] ?? null;
}

function decodeHtmlText(value) {
  return value
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}
