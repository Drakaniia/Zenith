// Browser profiles: pure helpers for the avatar button and the profile dialog. The profiles
// themselves live in the backend (profiles.rs); each one runs in its own Zenith process.
import type { ProfileEntry } from "./types";

export const MAX_PROFILE_NAME = 40;

/** The letter shown on a profile's avatar (first character, upper-cased; emoji kept whole). */
export function profileInitial(name: string): string {
  const first = Array.from(name.trim())[0] ?? "?";
  return first.toLocaleUpperCase();
}

/** Mirrors the backend's clean-up: one line, single spaces, at most 40 characters. */
export function cleanProfileName(name: string): string {
  const flat = name.replace(/[\u0000-\u001f\u007f]/g, " ").split(/\s+/).filter(Boolean).join(" ");
  return Array.from(flat).slice(0, MAX_PROFILE_NAME).join("").trim();
}

/** Text colour that stays readable on a `#rrggbb` background. */
export function textOn(color: string): string {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(color);
  if (!m) return "#fff";
  const [r, g, b] = m.slice(1).map((h) => {
    const c = parseInt(h, 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  const luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return luminance > 0.4 ? "#1a1a1a" : "#fff";
}

/** A colour for a new profile: the first one no profile uses yet. */
export function nextProfileColor(colors: string[], profiles: Pick<ProfileEntry, "color">[]): string {
  const used = new Set(profiles.map((p) => p.color.toLowerCase()));
  return colors.find((c) => !used.has(c.toLowerCase())) ?? colors[profiles.length % Math.max(colors.length, 1)] ?? "#8b7cf6";
}

/** Why a profile can't be deleted right now, or null when it can. */
export function deleteBlocker(profile: ProfileEntry, active: string): string | null {
  if (profile.isDefault) return "The Default profile can't be deleted.";
  if (profile.id === active) return "Switch to another profile to delete this one.";
  if (profile.running) return `Close the windows of “${profile.name}” to delete it.`;
  return null;
}
