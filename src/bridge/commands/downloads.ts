import type { DownloadItem } from '../../shared/types/backend';
import { invokeCommand } from '../invoke';

export function getDownloads(): Promise<DownloadItem[]> {
  return invokeCommand<DownloadItem[]>('get_downloads');
}

export function startDownload(url: string, filename: string): Promise<string> {
  return invokeCommand<string>('start_download', { url, filename });
}

export function cancelDownload(id: string): Promise<void> {
  return invokeCommand<void>('cancel_download', { id });
}

export function clearCompletedDownloads(): Promise<void> {
  return invokeCommand<void>('clear_completed_downloads');
}
