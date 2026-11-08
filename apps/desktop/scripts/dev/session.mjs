// Prints a summary of a Zenith session file (default: the test profile).
//   node scripts/dev/session.mjs [path\to\session.json]
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

const file =
  process.argv[2] ??
  join(tmpdir(), "zenith-test-profile", "roaming", "session.json");
const s = JSON.parse(readFileSync(file, "utf8"));
const time = (ms) => new Date(ms).toLocaleTimeString();
console.log(
  JSON.stringify(
    {
      spaces: s.spaces.map(
        (sp) =>
          `${sp.name}: ${sp.pinnedIds.length} pinned, ${sp.tabIds.length} tabs`,
      ),
      essentials: s.essentials.length,
      tabs: s.tabs.map(
        (t) =>
          `${t.pinned ? "[pinned] " : ""}${t.url.slice(0, 60)} (last active ${time(t.lastActive)})`,
      ),
      closed: s.closed.map(
        (c) => `${c.url.slice(0, 60)} closed ${time(c.closedAt)}`,
      ),
    },
    null,
    1,
  ),
);
