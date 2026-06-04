import {
  addHistoryEntry,
  clearHistory,
  deleteHistoryEntry,
  getHistory,
  searchHistory,
  type HistoryEntry
} from '../utils/tauri';
import { toastsState } from './toasts.svelte.js';

class HistoryState {
  entries = $state<HistoryEntry[]>([]);
  searchResults = $state<HistoryEntry[]>([]);
  query = $state('');
  isLoading = $state(false);
  isSearching = $state(false);
  count = $derived(this.entries.length);

  load = async (): Promise<void> => {
    this.isLoading = true;

    try {
      this.entries = await getHistory();
    } catch (error) {
      toastsState.error(`Failed to load history: ${errorMessage(error)}`);
    } finally {
      this.isLoading = false;
    }
  };

  addEntry = async (title: string, url: string): Promise<void> => {
    try {
      await addHistoryEntry(title, url);
      await this.load();
    } catch (error) {
      toastsState.error(`Failed to add history entry: ${errorMessage(error)}`);
    }
  };

  search = async (query: string): Promise<HistoryEntry[]> => {
    this.query = query;
    this.isSearching = true;

    try {
      const results = await searchHistory(query);
      this.searchResults = results;
      return results;
    } catch (error) {
      toastsState.error(`Failed to search history: ${errorMessage(error)}`);
      this.searchResults = [];
      return [];
    } finally {
      this.isSearching = false;
    }
  };

  removeEntry = async (id: string): Promise<void> => {
    try {
      await deleteHistoryEntry(id);
      this.entries = this.entries.filter((entry) => entry.id !== id);
      this.searchResults = this.searchResults.filter((entry) => entry.id !== id);
    } catch (error) {
      toastsState.error(`Failed to delete history entry: ${errorMessage(error)}`);
    }
  };

  clear = async (): Promise<void> => {
    try {
      await clearHistory();
      this.entries = [];
      this.searchResults = [];
      this.query = '';
    } catch (error) {
      toastsState.error(`Failed to clear history: ${errorMessage(error)}`);
    }
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

export const historyState = new HistoryState();
