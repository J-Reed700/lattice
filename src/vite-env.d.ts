/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_APP_TITLE?: string
  readonly MODE: string
  readonly DEV: boolean
  readonly PROD: boolean
  readonly SSR: boolean
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

interface LatticeBootGuard {
  markLoaded(): void
  markMounted(): void
  isMounted(): boolean
  disarm(): void
}

interface Window {
  __LATTICE_BOOT__?: LatticeBootGuard
}
