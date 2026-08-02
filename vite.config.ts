import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { viteStaticCopy } from "vite-plugin-static-copy";
import { fileURLToPath } from "node:url";

const mobileDevHost = process.env.TAURI_DEV_HOST;

export default defineConfig(({ mode }) => ({
  resolve: {
    alias: {
      "paperworks-e2e-bridge": fileURLToPath(
        new URL(
          mode === "e2e"
            ? "./src/e2eBridgeEnabled.ts"
            : "./src/e2eBridgeDisabled.ts",
          import.meta.url
        )
      )
    }
  },
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            {
              name: "pdfjs",
              priority: 20,
              test: /node_modules[\\/]pdfjs-dist/
            }
          ]
        }
      }
    }
  },
  plugins: [
    ...(mode === "e2e"
      ? [
          {
            name: "paperworks-e2e-boot-diagnostics",
            transformIndexHtml() {
              return [
                {
                  tag: "script",
                  injectTo: "head-prepend" as const,
                  children:
                    "window.__paperworksE2eBootErrors=[];window.addEventListener('error',function(event){window.__paperworksE2eBootErrors.push(String(event.message).slice(0,500));});window.addEventListener('unhandledrejection',function(event){window.__paperworksE2eBootErrors.push(String(event.reason).slice(0,500));});"
                }
              ];
            }
          }
        ]
      : []),
    react(),
    viteStaticCopy({
      targets: [
        {
          src: "node_modules/pdfjs-dist/cmaps",
          dest: "pdfjs",
          rename: { stripBase: 2 }
        },
        {
          src: "node_modules/pdfjs-dist/iccs",
          dest: "pdfjs",
          rename: { stripBase: 2 }
        },
        {
          src: "node_modules/pdfjs-dist/web/images",
          dest: "pdfjs",
          rename: { stripBase: 3 }
        },
        {
          src: "node_modules/pdfjs-dist/standard_fonts",
          dest: "pdfjs",
          rename: { stripBase: 2 }
        },
        {
          src: "node_modules/pdfjs-dist/wasm",
          dest: "pdfjs",
          rename: { stripBase: 2 }
        }
      ]
    })
  ],
  clearScreen: false,
  server: {
    host: mobileDevHost || "127.0.0.1",
    strictPort: true,
    port: 5173,
    hmr: mobileDevHost
      ? {
          protocol: "ws",
          host: mobileDevHost,
          port: 5174
        }
      : undefined
  },
  envPrefix: ["VITE_", "TAURI_"]
}));
