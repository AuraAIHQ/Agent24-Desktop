// IPC handler registry. M0/M1 starter handlers; capability modules will
// register their own handlers here via the module loader (later M1 task).

import { app, ipcMain, shell } from 'electron'
import { IpcChannels } from '../../shared/ipc-types'

// Allowlist of URL prefixes permitted for shell.openExternal. Anything
// not matching is rejected silently — prevents IPC injection opening
// file:// or other dangerous schemes.
const EXTERNAL_URL_ALLOWLIST = ['https://', 'http://'] as const

export function registerIpcHandlers(): void {
  ipcMain.handle(IpcChannels.AppPing, () => 'pong')
  ipcMain.handle(IpcChannels.AppVersion, () => app.getVersion())
  ipcMain.handle(IpcChannels.ShellOpenExternal, (_event, url: unknown) => {
    if (typeof url !== 'string') return
    const allowed = EXTERNAL_URL_ALLOWLIST.some((prefix) => url.startsWith(prefix))
    if (allowed) {
      shell.openExternal(url).catch((err) => console.error('openExternal failed', err))
    }
  })
}
