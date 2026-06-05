import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadProgress } from '../../shared/types/backend';

export const DOWNLOAD_PROGRESS_EVENT = 'download-progress';

export function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void
): Promise<UnlistenFn> {
  return listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) => {
    handler(event.payload);
  });
}
