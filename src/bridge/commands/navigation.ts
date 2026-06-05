import type { PageMetadata } from '../../shared/types/backend';
import { invokeCommand } from '../invoke';

export function normalizeUrl(url: string): Promise<string> {
  return invokeCommand<string>('normalize_url', { url });
}

export function fetchPageMetadata(url: string): Promise<PageMetadata> {
  return invokeCommand<PageMetadata>('fetch_page_metadata', { url });
}
