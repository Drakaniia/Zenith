/** Memory use of Zenith's processes (Settings › Tabs & memory). */

export interface MemoryStats {
  privateBytes: number;
  workingSet: number;
  systemLoad: number;
  processes: {
    pid: number;
    name: string;
    privateBytes: number;
    workingSet: number;
  }[];
}
