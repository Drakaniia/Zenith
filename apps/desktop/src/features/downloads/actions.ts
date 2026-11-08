import { registerActions } from "../../platform/actions";
import { ui } from "../../shared/ui/ui.svelte";
import { isWebUrl } from "../../shared/url";
import { liveTab } from "../browser/target";

registerActions({
  downloadMedia: (source) => {
    const live = liveTab(source);
    if (live && isWebUrl(live.url)) ui.openMedia(live.id);
    else ui.toast("Open a page with a video first", { icon: "film" });
  },
});
