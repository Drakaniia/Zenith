import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauriRuntime } from '../../bridge/runtime';

export async function focusAppShell(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await Promise.allSettled([getCurrentWindow().setFocus(), getCurrentWebview().setFocus()]);
}

export function focusAddressInput(): void {
  window.setTimeout(() => {
    void focusAppShell().finally(() => {
      const addressElement = document.querySelector<HTMLInputElement>('.address-input');
      addressElement?.focus();
      addressElement?.select();
    });
  }, 0);
}
