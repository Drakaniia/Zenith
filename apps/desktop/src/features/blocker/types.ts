/** The ad and tracker blocker's rule counts and filter lists. */

export interface BlockerStats {
  rules: number;
  totalBlocked: number;
  lists: {
    id: string;
    rules: number;
    updated?: number | null;
    error?: string | null;
  }[];
}
