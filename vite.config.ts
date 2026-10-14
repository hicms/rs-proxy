import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  resolve: { alias: {
    '@astryxdesign/theme-neutral/fonts.css': fileURLToPath(new URL('./src/theme-fonts.css', import.meta.url)),
    '@astryxdesign/theme-butter/fonts.css': fileURLToPath(new URL('./src/theme-fonts.css', import.meta.url)),
  } },
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: '127.0.0.1' },
  build: { target: 'es2022' },
});
