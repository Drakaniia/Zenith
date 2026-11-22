// Deny-by-default security prompts (certificate errors, local Safe Browsing blocks,
// external protocol launches, and dangerous download confirmations).
import { pageApi } from "./api";
import type {
  CertErrorRequest,
  DownloadWarningRequest,
  ExternalProtocolRequest,
  SafeBrowsingBlock,
} from "./types";

export type SecurityItem =
  | { kind: "cert"; data: CertErrorRequest }
  | { kind: "safebrowsing"; data: SafeBrowsingBlock }
  | { kind: "protocol"; data: ExternalProtocolRequest }
  | { kind: "download"; data: DownloadWarningRequest };

class SecurityStore {
  queue = $state<SecurityItem[]>([]);

  pushCert(data: CertErrorRequest) {
    this.queue = [...this.queue, { kind: "cert", data }];
  }

  pushSafeBrowsing(data: SafeBrowsingBlock) {
    this.queue = [
      ...this.queue.filter(
        (x) => !(x.kind === "safebrowsing" && x.data.tabId === data.tabId && x.data.url === data.url),
      ),
      { kind: "safebrowsing", data },
    ];
  }

  pushProtocol(data: ExternalProtocolRequest) {
    this.queue = [...this.queue, { kind: "protocol", data }];
  }

  pushDownload(data: DownloadWarningRequest) {
    this.queue = [...this.queue, { kind: "download", data }];
  }

  async answer(item: SecurityItem, allow: boolean) {
    this.queue = this.queue.filter((x) => x !== item);
    if (item.kind === "cert") {
      await pageApi.securityRespondCert(item.data.requestId, allow);
    } else if (item.kind === "protocol") {
      await pageApi.securityRespondProtocol(item.data.requestId, allow);
    } else if (item.kind === "download") {
      await pageApi.securityRespondDownload(item.data.requestId, allow);
    } else if (item.kind === "safebrowsing" && allow) {
      await pageApi.safebrowsingBypass(item.data.tabId, item.data.url);
    }
  }

  dropTab(tabId: string) {
    this.queue = this.queue.filter((x) => x.data.tabId !== tabId);
  }
}

export const security = new SecurityStore();
