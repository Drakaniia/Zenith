// Shared by the preview's fixtures: the page's query parameters and generated favicons.
export const params = new URLSearchParams(location.search);

export function favicon(letter: string, bg: string, fg = "#fff"): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="${bg}"/><text x="16" y="22" font-family="Segoe UI, Arial" font-size="17" font-weight="700" text-anchor="middle" fill="${fg}">${letter}</text></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}
