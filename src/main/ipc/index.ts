// IPC handler registry. M0/M1 starter handlers; capability modules will
// register their own handlers here via the module loader (later M1 task).

import { app, ipcMain } from 'electron'

export function registerIpcHandlers(): void {
  ipcMain.handle('app:ping', () => 'pong')
  ipcMain.handle('app:version', () => app.getVersion())
}
