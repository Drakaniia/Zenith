// Inline address-bar completion ("autofill"): typing "op" completes to "opencode.ai" from
// the user's own browsing first, then from a bundled list of popular sites.
// Self-contained (no runtime imports) so it runs under `node --test`.

/** A URL the user could be heading to, with how strongly it should win. */
export interface AutofillCandidate {
  url: string;
  weight: number;
}

export interface Autofill {
  /** The full text to show: what was typed plus the completed (selected) remainder. */
  text: string;
  /** Where Enter goes while the completion is showing. */
  url: string;
}

/** Popular sites, most popular first. Only used when nothing personal matches better. */
export const POPULAR_SITES: readonly string[] = [
  "google.com", "youtube.com", "facebook.com", "instagram.com", "wikipedia.org", "x.com",
  "twitter.com", "reddit.com", "whatsapp.com", "web.whatsapp.com", "amazon.com", "chatgpt.com",
  "openai.com", "tiktok.com", "linkedin.com", "netflix.com", "bing.com", "yahoo.com",
  "live.com", "outlook.live.com", "office.com", "microsoft.com", "github.com", "gmail.com",
  "mail.google.com", "drive.google.com", "docs.google.com", "maps.google.com",
  "calendar.google.com", "translate.google.com", "news.google.com", "photos.google.com",
  "duckduckgo.com", "twitch.tv", "discord.com", "spotify.com", "open.spotify.com",
  "pinterest.com", "ebay.com", "apple.com", "icloud.com", "music.apple.com", "zoom.us",
  "dropbox.com", "paypal.com", "imdb.com", "quora.com", "stackoverflow.com",
  "developer.mozilla.org", "medium.com", "notion.so", "figma.com", "canva.com", "slack.com",
  "trello.com", "atlassian.com", "gitlab.com", "bitbucket.org", "npmjs.com", "pypi.org",
  "crates.io", "docs.rs", "rust-lang.org", "python.org", "nodejs.org", "react.dev",
  "svelte.dev", "vuejs.org", "angular.dev", "typescriptlang.org", "vercel.com",
  "netlify.com", "cloudflare.com", "dash.cloudflare.com", "aws.amazon.com",
  "console.aws.amazon.com", "azure.microsoft.com", "portal.azure.com",
  "cloud.google.com", "console.cloud.google.com", "heroku.com", "digitalocean.com",
  "supabase.com", "firebase.google.com", "anthropic.com", "claude.ai", "gemini.google.com",
  "copilot.microsoft.com", "perplexity.ai", "huggingface.co", "opencode.ai", "cursor.com",
  "kaggle.com", "arxiv.org", "news.ycombinator.com", "producthunt.com", "dev.to",
  "hashnode.com", "substack.com", "tumblr.com", "bsky.app", "mastodon.social",
  "threads.net", "telegram.org", "web.telegram.org", "signal.org", "messenger.com",
  "snapchat.com", "vimeo.com", "soundcloud.com", "bandcamp.com", "deezer.com",
  "music.youtube.com", "primevideo.com", "disneyplus.com", "hulu.com", "max.com",
  "crunchyroll.com", "steampowered.com", "store.steampowered.com", "epicgames.com",
  "roblox.com", "minecraft.net", "ea.com", "xbox.com", "playstation.com", "nintendo.com",
  "aliexpress.com", "alibaba.com", "temu.com", "shein.com", "etsy.com", "walmart.com",
  "target.com", "bestbuy.com", "ikea.com", "booking.com", "airbnb.com", "tripadvisor.com",
  "expedia.com", "uber.com", "cnn.com", "bbc.com", "bbc.co.uk", "nytimes.com",
  "theguardian.com", "reuters.com", "bloomberg.com", "forbes.com", "weather.com",
  "espn.com", "theverge.com", "wired.com", "techcrunch.com", "arstechnica.com",
  "wordpress.com", "wix.com", "squarespace.com", "shopify.com", "stripe.com",
  "coinbase.com", "binance.com", "tradingview.com", "speedtest.net", "archive.org",
  "wolframalpha.com", "khanacademy.org", "coursera.org", "udemy.com", "duolingo.com",
  "chess.com", "lichess.org", "goodreads.com", "fandom.com", "opera.com", "mozilla.org",
  "brave.com", "proton.me", "mail.proton.me", "yandex.com", "ecosia.org", "search.brave.com",
];

const SCHEME = /^[a-z][a-z0-9+.-]*:\/\//i;

/** Autofill applies only to address-like input: one word, not a forced search. */
export function canAutofill(typed: string): boolean {
  return (
    typed.length > 0 &&
    !/\s/.test(typed) &&
    !typed.startsWith("?") &&
    !typed.startsWith(">") &&
    !/^[a-z]:[\\/]/i.test(typed) &&
    !typed.startsWith("\\\\")
  );
}

/**
 * The ways a URL can be typed, shortest first, each paired with where it leads.
 * With `withPath` false only the site is offered (Chrome/Firefox autofill the origin first).
 */
function forms(url: string, withPath: boolean): { text: string; url: string }[] {
  let u: URL;
  try {
    u = new URL(url);
  } catch {
    return [];
  }
  if (u.protocol !== "https:" && u.protocol !== "http:") return [];
  const host = u.host.toLowerCase();
  const bare = host.replace(/^www\./, "");
  if (!withPath) {
    const origin = `${u.protocol}//${host}/`;
    return [
      { text: bare, url: origin },
      { text: host === bare ? `www.${bare}` : host, url: origin },
      { text: `${u.protocol}//${host}`, url: origin },
    ];
  }
  const rest = (u.pathname === "/" && !u.search && !u.hash ? "" : u.pathname) + u.search + u.hash;
  return [
    { text: bare + rest, url: u.href },
    { text: host + rest, url: u.href },
    { text: `${u.protocol}//${host}${rest}`, url: u.href },
  ];
}

/**
 * Picks the best inline completion for `typed`, or null. Candidates are scored by summed
 * weight per completion (so a site visited through many pages wins); ties go to the
 * shorter completion. The typed characters keep their case; only the remainder is added.
 */
export function autofill(typed: string, candidates: readonly AutofillCandidate[]): Autofill | null {
  if (!canAutofill(typed)) return null;
  const lower = typed.toLowerCase();
  // Past the host ("github.com/sv"), complete whole URLs; before it, complete just the site.
  const afterScheme = lower.replace(SCHEME, "");
  const withPath = afterScheme.includes("/") || afterScheme.includes("?") || afterScheme.includes("#");
  const best = new Map<string, { text: string; url: string; weight: number }>();
  for (const c of candidates) {
    const hit = forms(c.url, withPath).find((f) => f.text.length > lower.length && f.text.startsWith(lower));
    if (!hit) continue;
    const prev = best.get(hit.text);
    if (prev) prev.weight += c.weight;
    else best.set(hit.text, { ...hit, weight: c.weight });
  }
  let win: { text: string; url: string; weight: number } | undefined;
  for (const b of best.values()) {
    if (!win || b.weight > win.weight || (b.weight === win.weight && b.text.length < win.text.length)) win = b;
  }
  if (!win) return null;
  return { text: typed + win.text.slice(typed.length), url: win.url };
}

/** The bundled sites as candidates, weighted below anything from the user's own browsing. */
export function popularCandidates(): AutofillCandidate[] {
  const n = POPULAR_SITES.length;
  return POPULAR_SITES.map((host, i) => ({ url: `https://${host}/`, weight: (n - i) / (n + 1) }));
}
