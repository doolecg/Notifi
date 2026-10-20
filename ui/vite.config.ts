import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
    rolldownOptions: {
      input: {
        main: fileURLToPath(new URL('index.html', import.meta.url)),
        panel: fileURLToPath(new URL('panel.html', import.meta.url)),
      },
    },
  },
});
