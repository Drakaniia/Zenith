import {
  cancelDownload,
  clearCompletedDownloads,
  getDownloads,
  startDownload
} from '../../../bridge/commands/downloads';
import type { DownloadItem, DownloadProgress } from '../../../shared/types/backend';
import { toastsState } from '../../../shell/state/toasts.svelte.js';

class DownloadsState {
  items = $state<DownloadItem[]>([]);
  isLoading = $state(false);
  isStarting = $state(false);
  activeDownloads = $derived(this.items.filter((item) => item.status === 'downloading'));
  completedDownloads = $derived(this.items.filter((item) => item.status === 'completed'));

  load = async (): Promise<void> => {
    this.isLoading = true;

    try {
      this.items = await getDownloads();
    } catch (error) {
      toastsState.error(`Failed to load downloads: ${errorMessage(error)}`);
    } finally {
      this.isLoading = false;
    }
  };

  start = async (url: string, filename: string): Promise<string | null> => {
    this.isStarting = true;

    try {
      const id = await startDownload(url, filename);
      await this.load();
      return id;
    } catch (error) {
      toastsState.error(`Failed to start download: ${errorMessage(error)}`);
      return null;
    } finally {
      this.isStarting = false;
    }
  };

  cancel = async (id: string): Promise<void> => {
    try {
      await cancelDownload(id);
      this.items = this.items.map((item) =>
        item.id === id && item.status !== 'completed' ? { ...item, status: 'failed' } : item
      );
    } catch (error) {
      toastsState.error(`Failed to cancel download: ${errorMessage(error)}`);
    }
  };

  clearCompleted = async (): Promise<void> => {
    try {
      await clearCompletedDownloads();
      this.items = this.items.filter((item) => item.status !== 'completed');
    } catch (error) {
      toastsState.error(`Failed to clear completed downloads: ${errorMessage(error)}`);
    }
  };

  applyProgress = (progress: DownloadProgress): void => {
    this.items = this.items.map((item) =>
      item.id === progress.id
        ? {
            ...item,
            total_bytes: progress.total_bytes,
            downloaded_bytes: progress.downloaded_bytes,
            status: progress.status
          }
        : item
    );
  };
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  return 'Unknown error';
}

export const downloadsState = new DownloadsState();
