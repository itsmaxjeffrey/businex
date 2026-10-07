# Performance improvements — 2026-10-07

## Measured startup payload

A fresh public demo desktop was inspected in the same browser before and after deployment:

| Startup JavaScript | Before | After | Reduction |
| --- | ---: | ---: | ---: |
| Decoded asset size | 613,825 bytes | 277,888 bytes | 54.7% |
| Compressed asset size | 167,684 bytes | 85,950 bytes | 48.7% |

The private app's published main asset measures 277,633 decoded bytes and 85,423 gzip bytes. These measurements describe the default dashboard startup. Opening additional modules loads their code as needed. Load times also depend on connection speed, cache state, and the device; byte reductions do not directly establish an elapsed-time multiplier.

## What changed

- Modules use React lazy loading. The approximately 293 KB terminal chunk is separate from startup JavaScript. Pointer hover and keyboard focus prefetch a module so it can open sooner.
- Module components are memoized, avoiding unnecessary module renders when window geometry changes.
- Pointer updates are grouped into animation frames. Layout persistence waits 200 ms after changes settle and flushes on page hide, reducing synchronous storage writes during dragging.
- Minimized windows retain mounted content and selected views. Windows restored as already minimized do not load their content until first shown.
- The server compresses suitable responses. Fingerprinted assets have a one-year immutable cache lifetime; HTML revalidates and business APIs explicitly use `no-store`.
- Module loading failures have a recovery screen instead of crashing the desktop.
- `npm run build:site` reproducibly stages the public demo. Previous fingerprinted demo assets remain available for already-open tabs.

## Verification

25 automated tests and 25 API smoke checks passed. A browser check verified module loading and preserved the CRM Companies view after minimizing and restoring its window. Live private-origin checks confirmed gzip asset delivery, immutable asset caching, HTML revalidation, and the API no-store boundary. Both deployments were updated, and a pre-upgrade database backup passed SQLite integrity checking.

## Measuring future changes

Use `npm run build` to compare startup and module sizes, and `npm test` plus `npm run smoke` to check behavior. Compare elapsed load times under the same cache, network and device conditions. Before optimizing large workspaces, profile actual queries and rendering, then add pagination or list virtualization where the measurements justify it.
