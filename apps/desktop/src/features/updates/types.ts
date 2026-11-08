/** App update state, as reported by the backend (`update-status`). */
export interface UpdateStatus {
  state: "idle" | "checking" | "downloading" | "ready" | "upToDate" | "error";
  currentVersion: string;
  version?: string | null;
  notes?: string | null;
  progress?: number | null;
  error?: string | null;
  checked?: number | null;
}
