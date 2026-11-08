// The AI chatbot of Ctrl+Alt+X (Firefox's AI chatbot sidebar, which Zen keeps): the chosen
// provider's site, shown in split view beside the current page. The first use asks which one.
import { browser } from "../browser/browser.svelte";
import { settings } from "../settings/settings.svelte";
import { ui } from "../../shared/ui/ui.svelte";
import { hostOf } from "../../shared/url";
import type { TabId } from "../../shared/types";

export interface Chatbot {
  name: string;
  url: string;
}

/** Firefox's providers, in its order. */
export const CHATBOTS: Record<string, Chatbot> = {
  chatgpt: { name: "ChatGPT", url: "https://chatgpt.com/" },
  claude: { name: "Claude", url: "https://claude.ai/new" },
  copilot: { name: "Copilot", url: "https://copilot.microsoft.com/" },
  gemini: { name: "Gemini", url: "https://gemini.google.com/" },
  huggingchat: { name: "HuggingChat", url: "https://huggingface.co/chat/" },
  lechat: { name: "Le Chat Mistral", url: "https://chat.mistral.ai/chat" },
};

/** Width share of the page when the chatbot is beside it. */
const PAGE_SHARE = 0.68;

let chatTabId: TabId | null = null;

/** The chatbot's tab, if it is still open: the one this window opened, else any tab of the
 *  provider's site in the active space. */
function chatTab(bot: Chatbot): TabId | null {
  if (chatTabId && browser.tabs[chatTabId]) return chatTabId;
  const host = hostOf(bot.url);
  const space = browser.activeSpace;
  const found = space?.tabIds.find((id) => hostOf(browser.tabs[id]?.url ?? "") === host);
  return (chatTabId = found ?? null);
}

/** Opens the chatbot beside the current page, or hides it when it is showing. */
export function toggleChatbot() {
  const bot = CHATBOTS[settings.value.chatbot];
  if (!bot) return chooseChatbot(true);
  const existing = chatTab(bot);
  const split = browser.activeSplit;
  if (existing && split?.ids.includes(existing)) {
    browser.unsplit();
    const other = split.ids.find((id) => id !== existing);
    if (other) browser.activate(other);
    return;
  }
  const current = browser.activeTab;
  const id = existing ?? browser.openTab(bot.url, { activate: false }).id;
  chatTabId = id;
  if (!current || current.id === id) return browser.activate(id);
  browser.splitTabs([current.id, id], "vsep");
  const s = browser.activeSplit;
  if (s) s.ratio = PAGE_SHARE;
  browser.activate(id);
}

/** Asks which chatbot to use (a menu near the top of the window); `open` opens it afterwards. */
export function chooseChatbot(open = false) {
  ui.openMenu(
    Math.max(8, window.innerWidth / 2 - 120),
    56,
    [
      { label: "Choose an AI chatbot", heading: true },
      ...Object.entries(CHATBOTS).map(([id, bot]) => ({
        label: bot.name,
        checked: settings.value.chatbot === id,
        run: () => {
          const changed = settings.value.chatbot !== id;
          settings.update({ chatbot: id });
          if (changed) chatTabId = null;
          if (open) toggleChatbot();
        },
      })),
    ],
  );
}
