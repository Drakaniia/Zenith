// Loads every feature's actions into the registry (platform/actions.ts), so shortcuts, menus and
// palette commands can run them by id. Imported once by main.ts before the UI mounts.
import { blockActionsWhile } from "../platform/actions";
import { ui } from "../shared/ui/ui.svelte";
import "../features/bookmarks/actions";
import "../features/browser/actions";
import "../features/chatbot/actions";
import "../features/downloads/actions";
import "../features/notes/actions";
import "../features/onboarding/actions";
import "../features/page/actions";
import "../features/palette/actions";
import "../features/sidebar/actions";
import "../features/window/actions";

// While the shortcut editor records a key, the key must not also run its action.
blockActionsWhile(() => ui.shortcutRecording);
