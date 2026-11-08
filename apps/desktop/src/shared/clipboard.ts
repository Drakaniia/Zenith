// Copying text, with the backend's clipboard as the fallback when the web API is refused.
import { shell } from "../platform/shell";

export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    await shell.clipboardWrite(text);
  }
}
