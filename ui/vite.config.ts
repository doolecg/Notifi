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
        apps: fileURLToPath(new URL('apps.html', import.meta.url)),
        main: fileURLToPath(new URL('index.html', import.meta.url)),
        panel: fileURLToPath(new URL('panel.html', import.meta.url)),
        island: fileURLToPath(new URL('island.html', import.meta.url)),
        clipboard: fileURLToPath(new URL('clipboard.html', import.meta.url)),
        snippets: fileURLToPath(new URL('snippets.html', import.meta.url)),
        shelf: fileURLToPath(new URL('shelf.html', import.meta.url)),
        snap: fileURLToPath(new URL('snap.html', import.meta.url)),
        commandbar: fileURLToPath(new URL('commandbar.html', import.meta.url)),
        osd: fileURLToPath(new URL('osd.html', import.meta.url)),
        quick: fileURLToPath(new URL('quick.html', import.meta.url)),
        scratchpad: fileURLToPath(new URL('scratchpad.html', import.meta.url)),
        picker: fileURLToPath(new URL('picker.html', import.meta.url)),
        recording: fileURLToPath(new URL('recording.html', import.meta.url)),
        camera: fileURLToPath(new URL('camera.html', import.meta.url)),
        capture: fileURLToPath(new URL('capture.html', import.meta.url)),
        mediatools: fileURLToPath(new URL('mediatools.html', import.meta.url)),
        switcher: fileURLToPath(new URL('switcher.html', import.meta.url)),
        taskbar: fileURLToPath(new URL('taskbar.html', import.meta.url)),
        radial: fileURLToPath(new URL('radial.html', import.meta.url)),
      },
    },
  },
});
