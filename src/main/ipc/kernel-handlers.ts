// IPC handlers for kernel module management (M1 Task 10).

import { ipcMain } from 'electron'
import { IpcChannels } from '../../shared/ipc-types'
import { getKernel } from '../kernel'

export function registerKernelHandlers(): void {
  // Returns a plain array of { id, state } records — safe to send over IPC.
  ipcMain.handle(IpcChannels.KernelListModules, () => {
    const k = getKernel()
    return [...k.list().entries()].map(([id, r]) => ({ id, state: r.state }))
  })

  // Load a single module by id.
  ipcMain.handle(IpcChannels.KernelLoadModule, (_event, id: unknown) => {
    if (typeof id !== 'string') {
      throw new Error('KernelLoadModule: id must be a string')
    }
    return getKernel().load(id)
  })

  // Invoke an intent on a loaded module.
  ipcMain.handle(
    IpcChannels.KernelInvoke,
    (_event, id: unknown, intent: unknown) => {
      if (typeof id !== 'string') {
        throw new Error('KernelInvoke: id must be a string')
      }
      if (
        typeof intent !== 'object' ||
        intent === null ||
        typeof (intent as Record<string, unknown>)['kind'] !== 'string'
      ) {
        throw new Error('KernelInvoke: intent must be { kind: string; payload: unknown }')
      }
      return getKernel().invoke(id, intent as { kind: string; payload: unknown })
    },
  )
}
