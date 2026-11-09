// Windows shell services: file pickers, opening files and folders, the clipboard.
import { invoke } from "@tauri-apps/api/core";

export const shell = {
  /** A local file chosen in the system dialog (Ctrl+O), or null. */
  pickFile: () => invoke<string | null>("pick_file"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  showInFolder: (path: string) => invoke<void>("show_in_folder", { path }),
  pickFolder: (title?: string) =>
    invoke<string | null>("pick_folder", { title: title ?? null }),
  clipboardWrite: (text: string) => invoke<void>("clipboard_write", { text }),
  /** The clipboard's text, for the chrome's own Paste menu items. */
  clipboardRead: () => invoke<string>("clipboard_read"),
};
