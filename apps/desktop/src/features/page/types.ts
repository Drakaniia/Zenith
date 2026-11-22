import type { TabId } from "../browser/types";

export interface PermissionRequest {
  requestId: number;
  tabId: TabId;
  origin: string;
  kind: string;
  userInitiated: boolean;
}

export interface CertErrorRequest {
  requestId: number;
  tabId: TabId;
  host: string;
  url: string;
  error: string;
}

export interface SafeBrowsingBlock {
  tabId: TabId;
  url: string;
  host: string;
  category: string;
}

export interface ExternalProtocolRequest {
  requestId: number;
  tabId: TabId;
  scheme: string;
  url: string;
}

export interface DownloadWarningRequest {
  requestId: number;
  tabId: TabId;
  fileName: string;
  url: string;
  reason: string;
}

export interface SafeBrowsingStatus {
  enabled: boolean;
  entries: number;
  updated?: number | null;
  feeds: string[];
}
