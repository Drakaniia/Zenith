// Live start-page state shared by every new tab: the preferences (kept in settings.json), the
// "cleared" view entered by holding the pointer over the settings button, uploaded backgrounds
// (stored on disk by the backend, cached here), and the daily background change.
import { api } from "$platform/ipc";
import { settings } from "$features/settings/settings.svelte";
import { GALLERY_IDS, galleryById, galleryUrl } from "./wallpapers";
import {
  MAX_CUSTOM_IMAGES,
  MAX_HIDDEN_SITES,
  UPLOAD_MAX_SIDE,
  averageLuminance,
  dailyChoice,
  defaultStartPage,
  fitWithin,
  localDay,
  moveItem,
  newSiteId,
  newWallpaperId,
  siteKey,
  validateSite,
  parseBackground,
  toneOf,
  type StartPageSettings,
  type Tone,
} from "./startpage";

export interface ResolvedBackground {
  url: string;
  tone: Tone;
}

const UPLOAD_TYPES = /^image\/(jpeg|png|webp|gif|bmp|avif)$/;
const UPLOAD_MAX_FILE = 40 * 1024 * 1024;

class StartPageStore {
  /** Search, tiles, greeting and hints are hidden for a clean view (not saved). */
  cleared = $state(false);
  /** The saved-site form shown over the start page: a new site (no id) or one being edited. */
  editing = $state<{ id?: string; url: string; title: string } | null>(null);
  /** Uploaded backgrounds loaded from disk, by id. */
  images = $state<Record<string, string>>({});
  #loading = new Set<string>();

  get prefs(): StartPageSettings {
    return settings.value.startPage;
  }

  set(patch: Partial<StartPageSettings>) {
    Object.assign(settings.value.startPage, patch);
    settings.save(false);
  }

  /** Put the layout options back to their defaults; saved sites and background choices stay. */
  resetLayout() {
    const d = defaultStartPage();
    const { sites, hiddenSites, background, customImages, dailyBackground, dailyDate, dim, blur } = this.prefs;
    settings.value.startPage = { ...d, sites, hiddenSites, background, customImages, dailyBackground, dailyDate, dim, blur };
    settings.save(false);
  }

  /** Saves the open site form. Returns the problem to show, or "" when it was saved. */
  saveSite(input: { url: string; title: string }): string {
    const id = this.editing?.id;
    const sites = this.prefs.sites;
    const r = validateSite(input, sites, id);
    if ("error" in r) return r.error;
    const next = id
      ? sites.map((s) => (s.id === id ? { ...s, ...r } : s))
      : [...sites, { id: newSiteId(Date.now(), Math.random()), ...r }];
    // A site the user saves on purpose shouldn't stay hidden from most visited.
    const key = siteKey(r.url);
    this.set({ sites: next, hiddenSites: this.prefs.hiddenSites.filter((h) => h !== key), showSaved: true });
    this.editing = null;
    return "";
  }

  removeSite(id: string) {
    this.set({ sites: this.prefs.sites.filter((s) => s.id !== id) });
  }

  moveSite(id: string, to: number) {
    const sites = this.prefs.sites;
    this.set({ sites: moveItem(sites, sites.findIndex((s) => s.id === id), to) });
  }

  /** Removes a most visited site from the Tableau (until "Restore"). */
  hideSite(url: string) {
    const key = siteKey(url);
    if (!key || this.prefs.hiddenSites.includes(key)) return;
    this.set({ hiddenSites: [...this.prefs.hiddenSites, key].slice(-MAX_HIDDEN_SITES) });
  }

  restoreHidden() {
    this.set({ hiddenSites: [] });
  }

  apply(ref: string) {
    this.set({ background: ref, dailyDate: localDay(new Date()) });
  }

  /** Loads an uploaded background from disk once. */
  ensureImage(id: string) {
    if (this.images[id] || this.#loading.has(id)) return;
    this.#loading.add(id);
    api
      .wallpaperLoad(id)
      .then((data) => {
        if (data) this.images[id] = data;
      })
      .catch((e) => console.warn("background could not be read", e))
      .finally(() => this.#loading.delete(id));
  }

  /** The picture behind `ref` with its brightness, or null for none (or not loaded yet). */
  resolve(ref: string): ResolvedBackground | null {
    const b = parseBackground(ref);
    if (b.kind === "gallery") {
      const url = galleryUrl(b.id);
      return url ? { url, tone: galleryById(b.id)!.tone } : null;
    }
    if (b.kind === "custom") {
      const img = this.prefs.customImages.find((c) => c.id === b.id);
      if (!img) return null;
      this.ensureImage(b.id);
      const url = this.images[b.id];
      return url ? { url, tone: img.tone } : null;
    }
    return null;
  }

  /** With "new background every day" on, move to the day's picture the first time it is opened. */
  rollDaily() {
    const p = this.prefs;
    if (!p.dailyBackground) return;
    const day = localDay(new Date());
    if (p.dailyDate === day) return;
    this.set({ background: dailyChoice(GALLERY_IDS, p.customImages, day) || p.background, dailyDate: day });
  }

  /** Downscales an image file, stores it and adds it to "Your images". Returns its background ref. */
  async upload(file: File): Promise<string> {
    if (this.prefs.customImages.length >= MAX_CUSTOM_IMAGES)
      throw new Error(`You can keep up to ${MAX_CUSTOM_IMAGES} of your own images. Remove one to add another.`);
    if (!UPLOAD_TYPES.test(file.type)) throw new Error("Choose a JPEG, PNG, WebP, GIF or AVIF image.");
    if (file.size > UPLOAD_MAX_FILE) throw new Error("This image is larger than 40 MB. Choose a smaller one.");
    let bitmap: ImageBitmap;
    try {
      bitmap = await createImageBitmap(file);
    } catch {
      throw new Error("This file couldn't be opened as an image.");
    }
    const [w, h] = fitWithin(bitmap.width, bitmap.height, UPLOAD_MAX_SIDE);
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    canvas.getContext("2d")!.drawImage(bitmap, 0, 0, w, h);
    const data = canvas.toDataURL("image/jpeg", 0.86);

    // Brightness of the middle band, where the search bar and tiles sit.
    const probe = document.createElement("canvas");
    probe.width = probe.height = 24;
    const pc = probe.getContext("2d", { willReadFrequently: true })!;
    pc.drawImage(bitmap, 0, bitmap.height * 0.2, bitmap.width, bitmap.height * 0.6, 0, 0, 24, 24);
    const tone = toneOf(averageLuminance(pc.getImageData(0, 0, 24, 24).data));
    bitmap.close();

    const id = newWallpaperId(Date.now(), Math.random());
    try {
      await api.wallpaperSave(id, data);
    } catch (e) {
      throw new Error(typeof e === "string" ? e : "The image couldn't be saved.");
    }
    this.images[id] = data;
    this.set({ customImages: [...this.prefs.customImages, { id, tone }] });
    return `custom:${id}`;
  }

  async removeImage(id: string) {
    const p = this.prefs;
    this.set({
      customImages: p.customImages.filter((c) => c.id !== id),
      background: p.background === `custom:${id}` ? "" : p.background,
    });
    delete this.images[id];
    await api.wallpaperDelete(id).catch((e) => console.warn("background could not be deleted", e));
  }
}

export const startPage = new StartPageStore();
