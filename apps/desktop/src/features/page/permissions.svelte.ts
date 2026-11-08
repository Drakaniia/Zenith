// Site permission requests (camera, location, notifications, …) waiting for an answer.
import type { PermissionRequest } from "./types";
import { pageApi } from "./api";

class PermissionStore {
  queue = $state<PermissionRequest[]>([]);
  push(p: PermissionRequest) {
    this.queue = [...this.queue, p];
  }
  async answer(p: PermissionRequest, allow: boolean, remember: boolean) {
    this.queue = this.queue.filter((x) => x.requestId !== p.requestId);
    await pageApi.permissionRespond(p.requestId, allow, remember);
  }
  dropTab(tabId: string) {
    this.queue = this.queue.filter((x) => x.tabId !== tabId);
  }
}

export const permissions = new PermissionStore();
