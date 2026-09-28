// SvelteKit app-level ambient types.
// See https://svelte.dev/docs/kit/types#app.d.ts for documentation.

declare global {
  // Build-time app version injected by vite.config.js `define` (the UI must
  // not import package.json at runtime — Vite won't serve it in dev).
  const __APP_VERSION__: string;

  namespace App {
    // interface Error {}
    // interface Locals {}
    // interface PageData {}
    // interface PageState {}
    // interface Platform {}
  }
}

export {};
