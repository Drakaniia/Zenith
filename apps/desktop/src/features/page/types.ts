import type { TabId } from "../browser/types";

export interface PermissionRequest {
  requestId: number;
  tabId: TabId;
  origin: string;
  kind: string;
  userInitiated: boolean;
}
