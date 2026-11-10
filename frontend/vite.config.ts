import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `base: "./"` so the built site works from any subpath (GitHub Pages
// project sites serve under /tactical-gear-builder/).
export default defineConfig({
  plugins: [react()],
  base: "./",
  build: {
    outDir: "dist",
  },
});
