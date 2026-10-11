import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const devHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
  },
  server: {
    port: 1420,
    strictPort: true,
    host: devHost ?? false,
    watch: {
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
  },
});
