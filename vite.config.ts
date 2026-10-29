import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/target/**', '**/.tools/**'] } },
  test: { environment: 'jsdom', restoreMocks: true, include: ['src/**/*.test.{ts,tsx}'] },
});
