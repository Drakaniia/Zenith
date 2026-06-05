export type FindDirection = 'next' | 'previous';
export type BrowserWebviewLoadState = 'started' | 'finished';

export interface BrowserWebviewNavigationEvent {
  label: string;
  url: string;
}

export interface BrowserWebviewTitleEvent {
  label: string;
  title: string;
}

export interface BrowserWebviewLoadEvent {
  label: string;
  url: string;
  state: BrowserWebviewLoadState;
}
