// Torrents in the UI: adding one (magnet link, .torrent file, dropped file), the file picker
// that is open, and playing a file while it downloads.
import { browser } from '$features/browser/browser.svelte';
import { ui } from '$shared/ui/ui.svelte';
import { downloadsApi } from './api';
import { findMagnet, isTorrentName } from './torrent';

class TorrentStore {
  /** The torrent whose file picker is open. */
  pickerId = $state<number | null>(null);

  openPicker(id: number) {
    this.pickerId = id;
  }

  closePicker() {
    this.pickerId = null;
  }

  async #run(fn: () => Promise<unknown>): Promise<boolean> {
    try {
      await fn();
      return true;
    } catch (e) {
      ui.toast(String(e), { icon: 'alert', timeout: 6000 });
      return false;
    }
  }

  /** Adds the magnet link in `text` (pasted or dropped). */
  addMagnet(text: string): Promise<boolean> {
    const uri = findMagnet(text);
    if (!uri) {
      ui.toast('That is not a magnet link. Magnet links start with magnet:?', { icon: 'alert' });
      return Promise.resolve(false);
    }
    return this.#run(() => downloadsApi.torrentAddMagnet(uri));
  }

  /** Adds dropped .torrent files. */
  async addFiles(files: Iterable<File>): Promise<void> {
    for (const file of files) {
      if (!isTorrentName(file.name)) {
        ui.toast(`${file.name} is not a .torrent file`, { icon: 'alert' });
        continue;
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      await this.#run(() => downloadsApi.torrentAddBytes(Array.from(bytes)));
    }
  }

  /** Asks for a .torrent file and adds it. */
  openFile(): Promise<boolean> {
    return this.#run(() => downloadsApi.torrentOpenFile());
  }

  /** Opens a file of the torrent in a new tab; it plays while the rest downloads. */
  play(id: number, file: number): Promise<boolean> {
    return this.#run(async () => {
      const url = await downloadsApi.torrentStreamUrl(id, file);
      browser.openTab(url, { activate: true });
    });
  }

  /** Downloads only the chosen files (starts a torrent that waited for the choice). */
  choose(id: number, files: number[]): Promise<boolean> {
    return this.#run(() => downloadsApi.torrentChooseFiles(id, files));
  }
}

export const torrents = new TorrentStore();
