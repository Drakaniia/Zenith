import { registerActions } from "../../platform/actions";
import { ui } from "../../shared/ui/ui.svelte";
import { incognito } from "../window/window.svelte";
import { onboarding } from "./onboarding.svelte";

registerActions({
  runSetup: () => {
    // An incognito window saves nothing, so setup there would be lost.
    if (incognito)
      return ui.toast("Run setup from a normal window", { icon: "incognito" });
    return onboarding.restart();
  },
});
