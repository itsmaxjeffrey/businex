import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Dev and preview proxy /api to the platform API so HttpOnly session cookies
// stay same-origin; no CORS and no cookie flags change in development. The
// target is configurable so tests can point at a disposable local API and
// never at a deployed endpoint.
const apiOrigin = process.env.BUSINEX_API_ORIGIN ?? "http://127.0.0.1:8788";

export default defineConfig({
  plugins: [solid()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: apiOrigin,
        changeOrigin: false
      }
    }
  },
  preview: {
    port: 4174,
    proxy: {
      "/api": {
        target: apiOrigin,
        changeOrigin: false
      }
    }
  },
  build: {
    target: "es2022"
  }
});