import { defineConfig } from 'vite';
import { resolve } from 'node:path';

// Two pages: the bar that floats over the taskbar, and the notes window.
export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: 'es2021',
    rollupOptions: {
      input: { bar: resolve(__dirname, 'index.html'), notes: resolve(__dirname, 'notes.html'), picker: resolve(__dirname, 'picker.html'), ask: resolve(__dirname, 'ask.html') },
    },
  },
});
