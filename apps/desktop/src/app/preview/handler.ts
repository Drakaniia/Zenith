// The preview's command handlers: each answers the commands of its domain and returns
// UNHANDLED for the rest, so mock.ts can try them in turn.
export const UNHANDLED = Symbol("unhandled");

export type Handler = (cmd: string, a: Record<string, unknown>) => unknown;
