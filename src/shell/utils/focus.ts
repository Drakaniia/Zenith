import { getCurrentWebview } from '@tauri-apps/api/webview';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invokeCommand } from '../../bridge/invoke';
import { isElectronRuntime, isTauriRuntime } from '../../bridge/runtime';

export async function focusAppShell(): Promise<void> {
  if (isElectronRuntime()) {
    await invokeCommand<void>('focus_shell');
    return;
  }

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
