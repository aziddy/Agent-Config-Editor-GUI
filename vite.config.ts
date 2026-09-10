import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const tauriHost = process.env["TAURI_DEV_HOST"];

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "src") },
  },
  server: {
    port: 5177,
    strictPort: true,
    host: tauriHost ?? "localhost",
    hmr: tauriHost ? { protocol: "ws", host: tauriHost, port: 5178 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    outDir: "dist",
    emptyOutDir: true,
  },
  test: {
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    environment: "node",
  },
});
