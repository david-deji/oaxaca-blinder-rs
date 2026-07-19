#!/usr/bin/env node
// COI static server for the threaded-WASM byte-parity harness (0014-MERIDIAN, verification-benchmark D-6).
//
// Serves the repo tree with the two cross-origin-isolation headers that SharedArrayBuffer — and therefore
// wasm-bindgen-rayon's worker pool — require:
//   Cross-Origin-Opener-Policy: same-origin
//   Cross-Origin-Embedder-Policy: require-corp
// Without them, `crossOriginIsolated === false`, SharedArrayBuffer is undefined, and the threaded WASM
// silently falls back to single-threaded. That silent fallback is exactly what the harness's AC-3
// `crossOriginIsolated === true` assertion exists to catch (agentic failure mode #6). CORP:cross-origin is
// added so the wasm/js/csv sub-resources load under require-corp.
//
// Zero deps (Node builtins only) so CI serves without an npm install. Binds 127.0.0.1 only — this is an
// ephemeral localhost test server, never a tm-server bind (the port-allocation rule governs tm-server).
import { createServer } from 'node:http';
import { readFile, readdir, stat } from 'node:fs/promises';
import { join, normalize, extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = fileURLToPath(new URL('.', import.meta.url));
const DOC_ROOT = resolve(process.env.COI_ROOT || join(HERE, '..', '..')); // repo root by default
const PORT = Number(process.env.COI_PORT || 8787);

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json; charset=utf-8',
  '.csv': 'text/csv; charset=utf-8',
};

const server = createServer(async (req, res) => {
  // COI headers on EVERY response (including 404/500) so every sub-resource load stays isolated.
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
  res.setHeader('Cross-Origin-Resource-Policy', 'cross-origin');
  res.setHeader('Cache-Control', 'no-store');

  try {
    const urlPath = decodeURIComponent((req.url || '/').split('?')[0]);
    const rel = normalize(urlPath).replace(/^([/\\]|\.\.[/\\])+/, '');
    let filePath = resolve(join(DOC_ROOT, rel));
    if (filePath !== DOC_ROOT && !filePath.startsWith(DOC_ROOT + sep)) {
      res.writeHead(403); res.end('403 forbidden'); return; // path-traversal guard
    }
    let s = await stat(filePath).catch(() => null);
    if (s && s.isDirectory()) {
      const idx = join(filePath, 'index.html');
      const idxStat = await stat(idx).catch(() => null);
      if (idxStat) {
        filePath = idx; s = idxStat;
      } else {
        // Bundler-free resolution of a wasm-bindgen pkg-directory import: wasm-bindgen-rayon's
        // workerHelpers.js does `import('../../..')` → the pkg dir, expecting a bundler/Node to
        // resolve it to the JS entry. A plain static server must do that itself. Serve the `<name>.js`
        // whose sibling `<name>_bg.wasm` exists (the wasm-bindgen entry) — mirrors what Vite does in
        // the real app. This is the fix for the E3 nested-worker-spawn path under a raw server.
        const entries = await readdir(filePath).catch(() => []);
        const jsEntry = entries.find(
          (f) => f.endsWith('.js') && entries.includes(f.replace(/\.js$/, '_bg.wasm'))
        );
        if (jsEntry) { filePath = join(filePath, jsEntry); s = await stat(filePath).catch(() => null); }
        else { s = null; }
      }
    }
    if (!s) { res.writeHead(404); res.end('404 ' + rel); return; }
    const body = await readFile(filePath);
    res.setHeader('Content-Type', MIME[extname(filePath)] || 'application/octet-stream');
    res.writeHead(200); res.end(body);
  } catch (e) {
    res.writeHead(500); res.end('500 ' + (e && e.message));
  }
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`COI server listening: http://127.0.0.1:${PORT}/  (root=${DOC_ROOT})`);
});
