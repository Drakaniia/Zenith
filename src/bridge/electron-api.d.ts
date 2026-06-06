import type { BackendCommand } from './invoke';

export {};

declare global {
  interface AmpuiElectronBridge {
    platform: string;
    invoke<T>(command: BackendCommand, args?: Record<string, unknown>): Promise<T>;
    on<T>(channel: string, handler: (payload: T) => void): () => void;
    off<T>(channel: string, handler?: (payload: T) => void): void;
    appVersion(): Promise<string | null>;
    clearAllBrowsingData(): Promise<void>;
    focusShell(): Promise<void>;
    toggleDevtools(): Promise<void>;
  }

  interface Window {
    ampuiElectron?: AmpuiElectronBridge;
  }
}
