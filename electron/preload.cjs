const { contextBridge, ipcRenderer } = require('electron');

const listenerRegistry = new Map();

function invoke(command, args) {
  return ipcRenderer.invoke('ampui:invoke', command, args);
}

contextBridge.exposeInMainWorld('ampuiElectron', {
  platform: process.platform,

  invoke,

  on(channel, handler) {
    const listener = (_event, payload) => {
      handler(payload);
    };
    const namespacedChannel = `ampui:event:${channel}`;
    ipcRenderer.on(namespacedChannel, listener);
    rememberListener(channel, handler, listener);

    return () => {
      removeListener(channel, handler);
    };
  },

  off(channel, handler) {
    removeListener(channel, handler);
  },

  appVersion() {
    return invoke('get_app_version');
  },

  clearAllBrowsingData() {
    return invoke('clear_all_browsing_data');
  },

  focusShell() {
    return invoke('focus_shell');
  },

  toggleDevtools() {
    return invoke('toggle_devtools');
  }
});

function rememberListener(channel, handler, listener) {
  const bucket = listenerRegistry.get(channel) ?? new Map();
  bucket.set(handler, listener);
  listenerRegistry.set(channel, bucket);
}

function removeListener(channel, handler) {
  const bucket = listenerRegistry.get(channel);
  const namespacedChannel = `ampui:event:${channel}`;

  if (!bucket) {
    return;
  }

  if (handler) {
    const listener = bucket.get(handler);
    if (listener) {
      ipcRenderer.removeListener(namespacedChannel, listener);
      bucket.delete(handler);
    }
  } else {
    for (const listener of bucket.values()) {
      ipcRenderer.removeListener(namespacedChannel, listener);
    }
    bucket.clear();
  }

  if (bucket.size === 0) {
    listenerRegistry.delete(channel);
  }
}
