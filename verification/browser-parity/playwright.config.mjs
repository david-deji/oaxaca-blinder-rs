import { defineConfig } from '@playwright/test';

// The COI server (coi-server.mjs) supplies COOP/COEP so headless Chromium reports crossOriginIsolated===true
// and exposes SharedArrayBuffer. Serial workers=1 because each thread-count run needs a fresh page/module
// (wasm-bindgen-rayon's initThreadPool is once-per-instance).
export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.mjs',
  fullyParallel: false,
  workers: 1,
  timeout: 120000,
  reporter: [['list']],
  webServer: {
    command: 'node coi-server.mjs',
    url: 'http://127.0.0.1:8787/verification/browser-parity/',
    reuseExistingServer: !process.env.CI,
    timeout: 30000,
  },
  use: {
    baseURL: 'http://127.0.0.1:8787',
    headless: true,
  },
});
