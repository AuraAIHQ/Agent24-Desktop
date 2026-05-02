// Kernel singleton for Agent24-Desktop main process.
//
// Initialised once at app.whenReady() and shut down on app quit.
// Uses @auraaihq/core (createKernel) + @auraaihq/memory (createMemory).
//
// M1 limitation: @auraaihq/core and @auraaihq/memory ship as ESM-only
// "bundler" packages (package.json "exports" points at src/index.ts).
// The electron main-process tsconfig uses moduleResolution: "node" which
// doesn't understand the "exports" field — tsc therefore cannot resolve
// the types from these packages in the normal way.  We work around this
// by importing through the absolute src/index.ts path and suppressing the
// residual resolver warnings with @ts-ignore.  M2 will add a proper CJS
// build to each package and remove these annotations.

// @ts-ignore — M1 file: dep; moduleResolution:node can't follow "exports"
import { createKernel } from '@auraaihq/core'
// @ts-ignore — M1 file: dep; moduleResolution:node can't follow "exports"
import { createMemory } from '@auraaihq/memory'
import { app } from 'electron'
import path from 'node:path'

// M1: use `any` for the kernel type — @auraaihq/core exports `Kernel` as an
// ESM interface under "exports" which moduleResolution:node cannot see.
// Typed properly once M2 ships a CJS build for the package.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let kernel: any = null

/**
 * Creates and stores the kernel singleton.
 * Must be called after app.whenReady() so that app.getPath('userData') is
 * available.
 */
export function initKernel(): void {
  if (kernel !== null) {
    return
  }

  const dbPath = path.join(app.getPath('userData'), 'agent24.db')

  // @ts-ignore — M1 file: dep types; resolved correctly at runtime
  const memory = createMemory({ filename: dbPath })

  // @ts-ignore — M1 file: dep types; resolved correctly at runtime
  kernel = createKernel({
    memory,
    log: {
      debug: (msg: string, ...args: unknown[]) => console.debug('[kernel]', msg, ...args),
      info:  (msg: string, ...args: unknown[]) => console.info('[kernel]', msg, ...args),
      warn:  (msg: string, ...args: unknown[]) => console.warn('[kernel]', msg, ...args),
      error: (msg: string, ...args: unknown[]) => console.error('[kernel]', msg, ...args),
    },
    events: {
      onModuleLoaded: (id: string) => console.info('[kernel] module loaded:', id),
      onModuleUnloaded: (id: string) => console.info('[kernel] module unloaded:', id),
    },
  })

  console.info('[kernel] initialised — db:', dbPath)
}

/**
 * Returns the kernel singleton.
 * Throws if initKernel() has not been called yet.
 *
 * Return type is `any` in M1 because @auraaihq/core ships ESM-only ("exports"
 * field) which moduleResolution:node cannot resolve.  Typed as Kernel once M2
 * adds a CJS build to the package.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function getKernel(): any {
  if (kernel === null) {
    throw new Error('Kernel not initialised — call initKernel() first')
  }
  return kernel
}
