// The setup assistant's state. Choices are applied (and saved) as the user makes them, so the
// window behind the assistant previews them and a closed window resumes where it was left.
// `settings.onboarded` is set only when the user finishes, so the assistant reappears on the
// next launch until then, and never after, unless setup is run again from Settings.
import { browser } from "./browser.svelte";
import { settings, type Settings } from "./settings.svelte";
import { STEPS, STEP_FIELDS, clampStep, stepIndex, type StepId } from "./onboarding-flow";

type Snapshot = Record<string, unknown>;

class OnboardingStore {
  open = $state(false);
  index = $state(0);
  /** Direction of the last move (1 forward, -1 back), for the step transition. */
  dir = $state<1 | -1>(1);
  /** Furthest step reached; the progress arc can jump back to any step up to it. */
  reached = $state(0);
  /** Why the current step can't continue yet (shown next to the disabled button), or "". */
  hold = $state("");
  /** What was imported, for the summary on the last step. */
  imported = $state({ bookmarks: 0, history: 0, from: [] as string[] });
  step = $derived(STEPS[this.index]);
  #entry: Snapshot = {};

  /** The accent colour is the colour of the active (on first run, the only) space. */
  get accent(): string {
    return browser.activeSpace?.color ?? "#8b7cf6";
  }

  setAccent(color: string) {
    const space = browser.activeSpace;
    if (space) space.color = color;
  }

  set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  /** Opens the assistant, resuming at the step it was closed on. */
  begin() {
    const saved = stepIndex(settings.value.onboardingStep);
    this.index = saved > 0 ? saved : 0;
    this.reached = this.index;
    this.dir = 1;
    this.hold = "";
    this.#snapshot();
    this.open = true;
  }

  /** Runs setup again (from Settings or the command palette). */
  restart() {
    settings.update({ onboarded: false, onboardingStep: undefined });
    this.begin();
  }

  go(i: number) {
    const to = clampStep(i);
    if (to === this.index) return;
    this.dir = to > this.index ? 1 : -1;
    this.hold = "";
    this.index = to;
    this.reached = Math.max(this.reached, to);
    settings.update({ onboardingStep: STEPS[to].id });
    this.#snapshot();
  }

  goTo(id: StepId) {
    this.go(stepIndex(id));
  }

  next() {
    if (this.index === STEPS.length - 1) return this.finish();
    this.go(this.index + 1);
  }

  back() {
    this.go(this.index - 1);
  }

  /** Leaves an optional step without changing anything it owns. */
  skip() {
    if (!this.step.optional) return;
    const patch: Snapshot = {};
    for (const key of STEP_FIELDS[this.step.id]) {
      if (key === "accent") this.setAccent(this.#entry.accent as string);
      else patch[key] = this.#entry[key];
    }
    if (Object.keys(patch).length) settings.update(patch as Partial<Settings>);
    this.next();
  }

  finish() {
    settings.update({ onboarded: true, onboardedAt: Date.now(), onboardingStep: undefined });
    this.open = false;
    // A first window has no tabs yet: show the home page that was just chosen.
    if (settings.value.homepage && !browser.activeTab) browser.openTab(settings.value.homepage);
  }

  #snapshot() {
    const snap: Snapshot = {};
    for (const key of STEP_FIELDS[STEPS[this.index].id]) {
      snap[key] = key === "accent" ? this.accent : settings.value[key as keyof Settings];
    }
    this.#entry = snap;
  }
}

export const onboarding = new OnboardingStore();
