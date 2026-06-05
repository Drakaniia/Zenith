import type { HistoryEntry } from '../../shared/types/backend';
import { invokeCommand } from '../invoke';

export function getHistory(): Promise<HistoryEntry[]> {
  return invokeCommand<HistoryEntry[]>('get_history');
}

export function addHistoryEntry(title: string, url: string): Promise<void> {
  return invokeCommand<void>('add_history_entry', { title, url });
}

export function searchHistory(query: string): Promise<HistoryEntry[]> {
  return invokeCommand<HistoryEntry[]>('search_history', { query });
}

export function clearHistory(): Promise<void> {
  return invokeCommand<void>('clear_history');
}

export function deleteHistoryEntry(id: string): Promise<void> {
  return invokeCommand<void>('delete_history_entry', { id });
}
