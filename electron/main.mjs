import { app, BrowserWindow, ipcMain, shell, session } from 'electron';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { registerElectronCommands } from './backend/commands.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const DEV_SERVER_URL = process.env.VITE_DEV_SERVER_URL ?? 'http://localhost:1420';

let commandHandlers = null;

function isDev() {
  return !app.isPackaged && !process.argv.includes('--dist');
}

async function createWindow(options = {}) {
  const window = new BrowserWindow({
    width: options.width ?? 1200,
    height: options.height ?? 800,
    minWidth: 800,
    minHeight: 600,
    title: options.title ?? 'ampui',
    frame: false,
    show: false,
    backgroundColor: '#0f0f10',
    webPreferences: {
      preload: join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      webviewTag: true
    }
  });

  window.once('ready-to-show', () => {
    window.show();
  });

  if (isDev()) {
    const url = new URL(options.url ?? '/', DEV_SERVER_URL);
    await window.loadURL(url.toString());
    window.webContents.openDevTools({ mode: 'detach' });
  } else {
    const filePath = join(__dirname, '..', 'dist', 'index.html');
    await window.loadFile(filePath, {
      query: options.incognito ? { incognito: '1' } : undefined
    });
  }

  return window;
}

function emitToRenderers(channel, payload) {
  for (const window of BrowserWindow.getAllWindows()) {
    window.webContents.send(`ampui:event:${channel}`, payload);
  }
}

function clearAllBrowsingData() {
  return session.defaultSession.clearStorageData();
}

function configureWebContentsSecurity() {
  app.on('web-contents-created', (_event, contents) => {
    contents.setWindowOpenHandler(({ url }) => {
      void openExternalIfSafe(url);
      return { action: 'deny' };
    });

    contents.on('will-navigate', (event, url) => {
      if (contents.getType() === 'webview') {
        if (!isHttpOrHttpsUrl(url)) {
          event.preventDefault();
        }
        return;
      }

      if (!isAllowedShellNavigation(url)) {
        event.preventDefault();
        void openExternalIfSafe(url);
      }
    });

    contents.on('will-attach-webview', (event, webPreferences, params) => {
      if (!isHttpOrHttpsUrl(params.src)) {
        event.preventDefault();
        return;
      }

      delete webPreferences.preload;
      delete webPreferences.preloadURL;
      webPreferences.nodeIntegration = false;
      webPreferences.contextIsolation = true;
      webPreferences.sandbox = true;
    });

    contents.on('before-input-event', (event, input) => {
      if (contents.getType() !== 'webview') {
        return;
      }

      const command = nativeShortcutCommand(input);
      if (!command) {
        return;
      }

      event.preventDefault();
      emitToRenderers('browser-shortcut', { command });
    });
  });
}

function isAllowedShellNavigation(url) {
  try {
    const parsed = new URL(url);

    if (isDev()) {
      return parsed.origin === new URL(DEV_SERVER_URL).origin;
    }

    return parsed.protocol === 'file:';
  } catch {
    return false;
  }
}

function isHttpOrHttpsUrl(url) {
  try {
    const parsed = new URL(url);
    return parsed.protocol === 'http:' || parsed.protocol === 'https:';
  } catch {
    return false;
  }
}

async function openExternalIfSafe(url) {
  if (!isSafeExternalUrl(url)) {
    return;
  }

  await shell.openExternal(url);
}

function isSafeExternalUrl(url) {
  try {
    const parsed = new URL(url);
    return ['http:', 'https:', 'mailto:'].includes(parsed.protocol);
  } catch {
    return false;
  }
}

function nativeShortcutCommand(input) {
  const key = input.key?.toLocaleLowerCase();
  const primary = process.platform === 'darwin' ? input.meta : input.control;
  const onlyPrimary = primary && !input.alt;

  if (input.alt && !input.control && !input.meta) {
    if (key === 'arrowleft') {
      return 'back';
    }

    if (key === 'arrowright') {
      return 'forward';
    }
  }

  if (!onlyPrimary) {
    return null;
  }

  if (input.shift && key === 't') {
    return 'restore-tab';
  }

  if (input.shift && key === 'n') {
    return 'new-incognito-window';
  }

  if (input.shift && key === 'r') {
    return 'hard-reload';
  }

  if (input.shift && key === 'g') {
    return 'find-previous';
  }

  if (input.shift && key === 'w') {
    return 'close-window';
  }

  if (key === 't') {
    return 'new-tab';
  }

  if (key === 'n') {
    return 'new-window';
  }

  if (key === 'w') {
    return 'close-tab';
  }

  if (key === 'l' || key === 'k' || key === 'e') {
    return 'focus-address';
  }

  if (key === 'r') {
    return 'reload';
  }

  if (key === 'f') {
    return 'find';
  }

  if (key === 'g') {
    return 'find-next';
  }

  if (key === 'h') {
    return 'history';
  }

  if (key === 'j') {
    return 'downloads';
  }

  if (key === 'd') {
    return input.shift ? 'bookmark-all' : 'bookmark-page';
  }

  if (key === 'p') {
    return 'print';
  }

  if (key === '+' || key === '=') {
    return 'zoom-in';
  }

  if (key === '-') {
    return 'zoom-out';
  }

  if (key === '0') {
    return 'zoom-reset';
  }

  if (key === 'i' || key === 'c') {
    return input.shift ? 'devtools' : null;
  }

  if (key >= '1' && key <= '8') {
    return `tab-${key}`;
  }

  if (key === '9') {
    return 'last-tab';
  }

  if (key === 'tab') {
    return input.shift ? 'previous-tab' : 'next-tab';
  }

  return null;
}

app.whenReady().then(async () => {
  configureWebContentsSecurity();

  commandHandlers = registerElectronCommands({
    ipcMain,
    app,
    shell,
    dataDir: app.getPath('userData'),
    downloadsDir: app.getPath('downloads'),
    emit: emitToRenderers,
    getFocusedWindow: () => BrowserWindow.getFocusedWindow(),
    getAllWindows: () => BrowserWindow.getAllWindows(),
    createWindow,
    clearAllBrowsingData
  });

  await createWindow();

  app.on('activate', async () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      await createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

export { commandHandlers };
