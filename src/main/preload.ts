// Preload script — runs in the renderer's process before any renderer code,
// with access to Node APIs. Exposes a tightly scoped bridge to renderer via
// contextBridge. Capability modules will extend this surface in M1+.

import { contextBridge, ipcRenderer } from 'electron'
import { IpcChannels } from '../shared/ipc-types'

const api = {
  // Placeholder API — M1 will replace with real module-aware surface.
  ping: (): Promise<string> => ipcRenderer.invoke(IpcChannels.AppPing),
  getAppVersion: (): Promise<string> => ipcRenderer.invoke(IpcChannels.AppVersion),
  // Opens a URL in the system browser. Rejects non-http(s) URLs in main.
  openExternal: (url: string): Promise<void> => ipcRenderer.invoke(IpcChannels.ShellOpenExternal, url),
  // Kernel module management (M1 Task 10).
  kernel: {
    listModules: (): Promise<{ id: string; state: string }[]> =>
      ipcRenderer.invoke(IpcChannels.KernelListModules),
    loadModule: (id: string): Promise<unknown> =>
      ipcRenderer.invoke(IpcChannels.KernelLoadModule, id),
    invoke: (
      id: string,
      intent: { kind: string; payload: unknown },
    ): Promise<unknown> =>
      ipcRenderer.invoke(IpcChannels.KernelInvoke, id, intent),
  },
} as const

contextBridge.exposeInMainWorld('agent24', api)

export type Agent24API = typeof api
