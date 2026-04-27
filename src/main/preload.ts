// Preload script — runs in the renderer's process before any renderer code,
// with access to Node APIs. Exposes a tightly scoped bridge to renderer via
// contextBridge. Capability modules will extend this surface in M1+.

import { contextBridge, ipcRenderer } from 'electron'

const api = {
  // Placeholder API — M1 will replace with real module-aware surface.
  ping: (): Promise<string> => ipcRenderer.invoke('app:ping'),
  getAppVersion: (): Promise<string> => ipcRenderer.invoke('app:version'),
} as const

contextBridge.exposeInMainWorld('agent24', api)

export type Agent24API = typeof api
