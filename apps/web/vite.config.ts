import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@businex/shared": path.resolve(import.meta.dirname, "../../packages/shared/src/index.ts"),
    },
  },
  server: {
    port: 5199,
    strictPort: true,
    proxy: {
      "/api": { target: "http://127.0.0.1:8788", changeOrigin: true },
      "/ws": { target: "ws://127.0.0.1:8788", ws: true },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: false,
    chunkSizeWarningLimit: 900,
  },
});
