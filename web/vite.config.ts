import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The server address of `make server`; override when that port is taken.
// Node types are not a dependency for one variable.
declare const process: { env: Record<string, string | undefined> };
const backend = process.env.ANYQ_API ?? "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react()],
  server: { proxy: { "/v1": backend, "/health": backend } },
  build: { outDir: "dist" },
  test: { environment: "node" },
});
