// Turning the ad and tracker blocker off or on for one site.
import { blockerApi } from "./api";
import { tabsApi } from "../browser/api";
import type { Tab } from "../browser/types";
import { settings } from "../settings/settings.svelte";
import { ui } from "../../shared/ui/ui.svelte";
import { hostOf, isWebUrl, siteOf } from "../../shared/url";

export function toggleSiteBlocking(tab: Tab | undefined) {
  if (!tab || !isWebUrl(tab.url)) return;
  const site = siteOf(hostOf(tab.url));
  const list = settings.value.blockerDisabledSites;
  const disabled = list.includes(site);
  settings.value.blockerDisabledSites = disabled
    ? list.filter((s) => s !== site)
    : [...list, site];
  settings.save(false);
  void blockerApi
    .blockerSetDisabledSites(settings.value.blockerDisabledSites)
    .then(() => {
      if (tab.live) void tabsApi.tabCommand(tab.id, "reload");
    });
  ui.toast(
    disabled
      ? `Blocking turned on for ${site}`
      : `Blocking turned off for ${site}`,
    {
      icon: disabled ? "shieldCheck" : "shieldOff",
    },
  );
}
