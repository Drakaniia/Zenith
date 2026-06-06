import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadProgress } from '../../shared/types/backend';
import { getElectronBridge } from '../runtime';

export const DOWNLOAD_PROGRESS_EVENT = 'download-progress';

export function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void
): Promise<UnlistenFn> {
  const electron = getElectronBridge();
  if (electron) {
    return Promise.resolve(electron.on<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, handler));
  }

  return listen<DownloadProgress>(DOWNLOAD_PROGRESS_EVENT, (event) => {
    handler(event.payload);
  });
}
