import { UNHANDLED } from "./handler";

// Browser profiles, kept in memory.
const PREVIEW_PROFILE_COLORS = ["#8b7cf6", "#3b82f6", "#14b8a6", "#22c55e", "#f59e0b", "#f97316", "#ef4444", "#ec4899", "#64748b"];
export const previewProfiles = [
  { id: "default", name: "Default", color: "#8b7cf6", isDefault: true },
  { id: "work", name: "Work", color: "#3b82f6", isDefault: false },
];

/** Profile commands. */
export function handleProfiles(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "profiles_list":
      return {
        active: "default",
        profiles: previewProfiles.map((p) => ({ ...p, running: p.id === "default", shortcut: false })),
        colors: PREVIEW_PROFILE_COLORS,
      };
    case "profile_create": {
      const name = String(a.name).trim();
      if (!name) return Promise.reject("Give the profile a name.");
      const base = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "profile";
      let id = base;
      for (let n = 2; previewProfiles.some((p) => p.id === id) || id === "default"; n++) id = `${base}-${n}`;
      const p = { id, name, color: String(a.color), isDefault: false };
      previewProfiles.push(p);
      return { ...p };
    }
    case "profile_update": {
      const p = previewProfiles.find((x) => x.id === a.id);
      if (!p) return Promise.reject("That profile no longer exists.");
      p.name = String(a.name).trim();
      p.color = String(a.color);
      return { ...p };
    }
    case "profile_delete": {
      const i = previewProfiles.findIndex((x) => x.id === a.id);
      if (i <= 0) return Promise.reject("The Default profile can't be deleted.");
      previewProfiles.splice(i, 1);
      return null;
    }
    case "profile_open":
      return null;
    case "profile_shortcut":
      return "C:\\Users\\you\\Desktop\\Zenith.lnk";
    default:
      return UNHANDLED;
  }
}
