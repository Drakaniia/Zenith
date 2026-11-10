// The downloads list: WebView2 downloads and media downloader jobs, newest first.
import type { DownloadItem } from "./types";
import { downloadsApi } from "./api";
import { RateMeter } from "./rate";

class DownloadStore {
  items = $state<DownloadItem[]>([]);
  loaded = false;
  /** Bytes per second of the running downloads, by id (measured from their reports). */
  rates = $state<Record<number, number>>({});
  #meter = new RateMeter();

  /** Overall speed of everything downloading. */
  totalRate = $derived(this.items.reduce((a, d) => a + (d.state === "progress" && !d.paused ? (this.rates[d.id] ?? 0) : 0), 0));

  active = $derived(this.items.filter((d) => d.state === "progress"));
  progress = $derived.by(() => {
    const running = this.items.filter(
      (d) => d.state === "progress" && d.total > 0,
    );
    if (!running.length) return -1;
    const total = running.reduce((a, d) => a + d.total, 0);
    const got = running.reduce((a, d) => a + d.received, 0);
    return total ? got / total : -1;
  });

  async load() {
    if (this.loaded) return;
    this.loaded = true;
    const [history, active] = await Promise.all([
      downloadsApi.downloadsHistory(200),
      downloadsApi.downloadsActive(),
    ]);
    const map = new Map<number, DownloadItem>();
    for (const d of history) map.set(d.id, d);
    for (const d of active) map.set(d.id, d);
    this.items = [...map.values()].sort((a, b) => b.started - a.started);
  }

  upsert(d: DownloadItem) {
    if (d.state === "progress" && !d.paused) {
      this.rates[d.id] = this.#meter.sample(d.id, d.received, Date.now());
    } else {
      this.#meter.forget(d.id);
      if (d.id in this.rates) delete this.rates[d.id];
    }
    const i = this.items.findIndex((x) => x.id === d.id);
    if (i >= 0) this.items[i] = { ...this.items[i], ...d };
    else this.items = [d, ...this.items];
  }

  /** A WebView2 download is running in the tab (closing its webview would cancel it). Media and torrent
   *  downloader jobs run on their own and do not keep a tab awake. */
  busyTab(tabId: string) {
    return this.items.some(
      (d) => d.state === "progress" && d.tabId === tabId && (d.kind ?? "file") === "file",
    );
  }

  /** Fetches a failed download that can't resume once more; the new one takes its place. */
  async again(id: number) {
    await downloadsApi.downloadAgain(id);
    this.#meter.forget(id);
    this.items = this.items.filter((d) => d.id !== id);
  }

  async remove(id: number | null) {
    await downloadsApi.downloadsRemove(id);
    this.items =
      id === null
        ? this.items.filter((d) => d.state === "progress")
        : this.items.filter((d) => d.id !== id);
  }
}

export const downloads = new DownloadStore();
