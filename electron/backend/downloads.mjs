import { createWriteStream } from 'node:fs';
import { mkdir } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { randomUUID } from 'node:crypto';

const FALLBACK_FILENAME = 'download';

export function createDownloadManager({ downloadsDir, emit }) {
  const items = [];
  const controllers = new Map();

  return {
    getDownloads() {
      return [...items];
    },

    async startDownload(url, filename = FALLBACK_FILENAME) {
      await mkdir(downloadsDir, { recursive: true });

      const id = randomUUID();
      const item = {
        id,
        filename: safeFilename(filenameFromInput(filename, url)),
        url,
        total_bytes: null,
        downloaded_bytes: 0,
        status: 'downloading',
        started_at: new Date().toISOString()
      };

      items.unshift(item);
      emitProgress(item);

      const controller = new AbortController();
      controllers.set(id, controller);
      void runDownload(item, controller).finally(() => controllers.delete(id));

      return id;
    },

    cancelDownload(id) {
      controllers.get(id)?.abort();
      const item = items.find((candidate) => candidate.id === id);

      if (item && item.status === 'downloading') {
        item.status = 'cancelled';
        emitProgress(item);
      }
    },

    clearCompletedDownloads() {
      for (let index = items.length - 1; index >= 0; index -= 1) {
        if (items[index].status === 'completed') {
          items.splice(index, 1);
        }
      }
    }
  };

  async function runDownload(item, controller) {
    const targetPath = join(downloadsDir, item.filename);

    try {
      const response = await fetch(item.url, { signal: controller.signal });

      if (!response.ok || !response.body) {
        throw new Error(`download failed: ${response.status} ${response.statusText}`);
      }

      const totalHeader = response.headers.get('content-length');
      item.total_bytes = totalHeader ? Number(totalHeader) : null;
      const file = createWriteStream(targetPath);
      const reader = response.body.getReader();

      try {
        while (true) {
          const { done, value } = await reader.read();
          if (done) {
            break;
          }

          item.downloaded_bytes += value.byteLength;
          file.write(Buffer.from(value));
          emitProgress(item);
        }
      } finally {
        file.end();
      }

      item.status = 'completed';
      emitProgress(item);
    } catch (error) {
      item.status = controller.signal.aborted ? 'cancelled' : 'failed';
      emitProgress(item);
    }
  }

  function emitProgress(item) {
    emit?.('download-progress', {
      id: item.id,
      total_bytes: item.total_bytes,
      downloaded_bytes: item.downloaded_bytes,
      status: item.status
    });
  }
}

function filenameFromInput(filename, url) {
  if (filename && filename !== FALLBACK_FILENAME) {
    return filename;
  }

  try {
    const parsed = new URL(url);
    return basename(parsed.pathname) || parsed.hostname || FALLBACK_FILENAME;
  } catch {
    return FALLBACK_FILENAME;
  }
}

function safeFilename(filename) {
  const sanitized = filename
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_')
    .replace(/\s+/g, ' ')
    .replace(/^\.+/, '')
    .replace(/\.+$/, '')
    .trim();

  return sanitized || FALLBACK_FILENAME;
}
