// Per-site extension access status derived from manifest host patterns + active tab URL.
import type { ExtensionInfo } from "./types";

export interface SiteAccess {
  state: "allowed" | "on-click" | "restricted" | "none";
  label: string;
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Evaluates a Chrome extension match pattern (e.g. `<all_urls>`, `https://*.example.com/*`)
 *  against a parsed web URL. */
export function matchesHostPattern(pattern: string, rawUrl: string): boolean {
  if (!pattern || !rawUrl) return false;
  let u: URL;
  try {
    u = new URL(rawUrl);
  } catch {
    return false;
  }
  const scheme = u.protocol.replace(/:$/, "").toLowerCase();
  if (scheme !== "http" && scheme !== "https" && scheme !== "file") return false;
  if (pattern === "<all_urls>") return true;

  const sep = pattern.indexOf("://");
  if (sep < 0) return false;
  const pScheme = pattern.slice(0, sep).toLowerCase();
  const rest = pattern.slice(sep + 3);
  const slash = rest.indexOf("/");
  if (slash < 0) return false;
  const pHost = rest.slice(0, slash).toLowerCase();
  const pPath = rest.slice(slash);

  if (pScheme === "*") {
    if (scheme !== "http" && scheme !== "https") return false;
  } else if (pScheme !== scheme) {
    return false;
  }

  const host = u.hostname.toLowerCase();
  if (pHost !== "*") {
    if (pHost.startsWith("*.")) {
      const base = pHost.slice(2);
      if (host !== base && !host.endsWith(`.${base}`)) return false;
    } else if (pHost !== host) {
      return false;
    }
  }

  if (pPath === "/*" || pPath === "*") return true;
  const targetPath = `${u.pathname}${u.search}`;
  const re = new RegExp(`^${pPath.split("*").map(escapeRegex).join(".*")}$`);
  return re.test(targetPath);
}

/** Human-readable per-site permission status for Zen's unified extensions panel. */
export function siteAccessStatus(
  ext: Pick<ExtensionInfo, "enabled" | "hostPermissions" | "hasActiveTab">,
  activeUrl?: string | null,
): SiteAccess {
  if (!ext.enabled) return { state: "none", label: "Disabled" };
  const url = activeUrl?.trim() ?? "";
  if (!url || !/^https?:/i.test(url)) {
    return { state: "restricted", label: "Can't run on this page" };
  }
  const patterns = ext.hostPermissions ?? [];
  if (patterns.some((p) => matchesHostPattern(p, url))) {
    return { state: "allowed", label: "Can access this site" };
  }
  if (ext.hasActiveTab) {
    return { state: "on-click", label: "When you click" };
  }
  return { state: "none", label: "No access to this site" };
}
