// Backend calls for find in page, permission prompts, and security interstitials/prompts.
import { invoke } from "@tauri-apps/api/core";
import type {
  CertErrorRequest,
  DownloadWarningRequest,
  ExternalProtocolRequest,
  PermissionRequest,
  SafeBrowsingBlock,
  SafeBrowsingStatus,
} from "./types";

export const pageApi = {
  find: (
    id: string,
    op: "start" | "next" | "prev" | "stop",
    text = "",
    caseSensitive = false,
    wholeWord = false,
  ) => invoke<void>("find_in_page", { id, op, text, caseSensitive, wholeWord }),
  permissionRespond: (requestId: number, allow: boolean, remember: boolean) =>
    invoke<void>("permission_respond", { requestId, allow, remember }),
  securityRespondCert: (requestId: number, proceed: boolean) =>
    invoke<void>("security_respond_cert", { requestId, proceed }),
  securityRespondProtocol: (requestId: number, allow: boolean) =>
    invoke<void>("security_respond_protocol", { requestId, allow }),
  securityRespondDownload: (requestId: number, allow: boolean) =>
    invoke<void>("security_respond_download", { requestId, allow }),
  safebrowsingStatus: () => invoke<SafeBrowsingStatus>("security_safebrowsing_status"),
  safebrowsingUpdate: () => invoke<SafeBrowsingStatus>("security_safebrowsing_update"),
  safebrowsingBypass: (tabId: string, url: string) =>
    invoke<void>("security_safebrowsing_bypass", { tabId, url }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "permission-request": PermissionRequest;
    "find-result": { id: string; matches: number; active: number };
    "cert-error": CertErrorRequest;
    "safebrowsing-block": SafeBrowsingBlock;
    "external-protocol": ExternalProtocolRequest;
    "download-warning": DownloadWarningRequest;
  }
}
