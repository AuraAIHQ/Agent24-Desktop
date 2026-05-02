// Shared IPC channel names and payload types between main and renderer.
// Capability modules will extend this in M1+.

export const IpcChannels = {
  AppPing: 'app:ping',
  AppVersion: 'app:version',
  // Opens a URL in the system browser via shell.openExternal (main process).
  // Renderer must never open external URLs directly — always route via this.
  ShellOpenExternal: 'shell:open-external',
  // Kernel module management channels (M1 Task 10).
  KernelListModules: 'kernel:list-modules',
  KernelLoadModule: 'kernel:load-module',   // payload: { id: string }
  KernelInvoke: 'kernel:invoke',            // payload: { id: string, intent: { kind: string, payload: unknown } }
} as const

export type IpcChannel = typeof IpcChannels[keyof typeof IpcChannels]
