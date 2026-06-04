export type ToastKind = 'success' | 'error' | 'info';

export interface ToastItem {
  id: string;
  kind: ToastKind;
  message: string;
}

const TOAST_TIMEOUT_MS = 4000;

class ToastsState {
  items = $state<ToastItem[]>([]);

  timers = new Map<string, ReturnType<typeof setTimeout>>();

  success = (message: string): ToastItem => this.add('success', message);

  error = (message: string): ToastItem => this.add('error', message);

  info = (message: string): ToastItem => this.add('info', message);

  remove = (id: string): void => {
    const timer = this.timers.get(id);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(id);
    }

    this.items = this.items.filter((item) => item.id !== id);
  };

  add = (kind: ToastKind, message: string): ToastItem => {
    const item: ToastItem = {
      id: createId(),
      kind,
      message
    };

    this.items = [...this.items, item];
    this.timers.set(
      item.id,
      setTimeout(() => {
        this.remove(item.id);
      }, TOAST_TIMEOUT_MS)
    );

    return item;
  };
}

function createId(): string {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export const toastsState = new ToastsState();
