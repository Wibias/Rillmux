import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

const pkg = JSON.parse(
  readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), "package.json"),
    "utf8",
  ),
) as { version: string };

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: {
    "import.meta.env.VITE_APP_VERSION": JSON.stringify(pkg.version),
  },
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "sentry",
              test: /node_modules[\\/]@sentry[\\/]/,
              priority: 40,
            },
            {
              name: "react-core",
              test: /node_modules[\\/](?:react|react-dom|scheduler)[\\/]/,
              priority: 30,
            },
            {
              name: "tauri",
              test: /node_modules[\\/]@tauri-apps[\\/]/,
              priority: 20,
            },
            {
              name: "i18n",
              test: /node_modules[\\/](?:i18next|react-i18next)[\\/]/,
              priority: 20,
            },
            {
              name: "app-state",
              test: /node_modules[\\/](?:@tanstack[\\/]react-query|react-router|zustand)[\\/]/,
              priority: 10,
            },
          ],
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**", "**/legacy/**"],
    },
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts", "scripts/**/*.test.ts"],
  },
});
