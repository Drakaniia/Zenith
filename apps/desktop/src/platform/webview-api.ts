// Placement and focus of the page webviews (see layout.svelte.ts).
import { invoke } from "@tauri-apps/api/core";

export interface PaneSpec {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface LayoutSpec {
  panes: PaneSpec[];
  glance: PaneSpec | null;
  overlay: boolean;
  viewportW: number;
  viewportH: number;
}

export const webviewApi = {
  setLayout: (spec: LayoutSpec) => invoke<void>("set_layout", { spec }),
  focusView: (target: string | null) => invoke<void>("focus_view", { target }),
};
