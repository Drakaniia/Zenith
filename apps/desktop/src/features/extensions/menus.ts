// Context / gear menu for an extension (toolbar right-click and unified panel row menu).
import { browser } from "$features/browser/browser.svelte";
import { openInternal } from "$features/browser/target";
import { settings } from "$features/settings/settings.svelte";
import { incognito } from "$features/window/window.svelte";
import type { PopupAnchor } from "$features/browser/types";
import type { MenuItem } from "$shared/ui/menu";
import { extensions, storeUrl } from "./extensions.svelte";
import type { ExtensionInfo } from "./types";

export function extensionMenu(ext: ExtensionInfo, anchor?: PopupAnchor): MenuItem[] {
  const pinned = extensions.isPinned(ext.id);
  const items: MenuItem[] = [
    { heading: true, label: ext.version ? `${ext.name} (${ext.version})` : ext.name },
  ];

  if (ext.enabled && (ext.hasAction || ext.popupUrl)) {
    items.push({
      label: ext.popupUrl ? "Open popup" : "Run extension",
      icon: "puzzle",
      run: () => void extensions.open(ext, anchor),
    });
  }
  if (ext.enabled && ext.popupUrl) {
    items.push({
      label: "Inspect popup",
      icon: "code",
      run: () => void extensions.open(ext, anchor, { inspect: true }),
    });
  }
  if (ext.optionsUrl) {
    items.push({
      label: "Options",
      icon: "settings",
      run: () => {
        extensions.close();
        browser.openTab(ext.optionsUrl!);
      },
    });
  }

  items.push(
    { separator: true },
    {
      label: pinned ? "Unpin from toolbar" : "Pin to toolbar",
      icon: pinned ? "unpin" : "pin",
      run: () => extensions.togglePin(ext.id),
    },
    {
      label: "Allow in incognito",
      icon: "incognito",
      checked: ext.incognito,
      disabled: incognito || !settings.value.extensionsInIncognito,
      run: () => void extensions.setIncognito(ext, !ext.incognito),
    },
    {
      label: "Manage extension",
      icon: "settings",
      run: () => {
        extensions.close();
        openInternal("extensions");
      },
    },
  );

  if (ext.storeId) {
    items.push({
      label: "View in Chrome Web Store",
      icon: "external",
      run: () => {
        extensions.close();
        browser.openTab(storeUrl(ext.storeId!));
      },
    });
  }

  if (!ext.builtin) {
    items.push(
      { separator: true },
      {
        label: "Remove extension",
        icon: "trash",
        danger: true,
        run: () => {
          extensions.close();
          void extensions.remove(ext);
        },
      },
    );
  }

  return items;
}
