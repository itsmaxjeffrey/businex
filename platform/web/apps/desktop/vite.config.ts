import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// The dev server proxies /api to the platform API so HttpOnly session cookies
// stay same-origin; no CORS and no cookie flags change in development.
export default defineConfig({
  plugins: [solid()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8788",
        changeOrigin: false
      }
    }
  },
  build: {
    target: "es2022"
  }
});
