import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
export default defineConfig({
  testDir: './tests/ui', fullyParallel: false, workers: 1,
  outputDir: join(tmpdir(), 'rs-proxy-playwright'),
  use: { baseURL: 'http://127.0.0.1:1420', viewport: { width: 1280, height: 820 }, channel: 'msedge', trace: 'retain-on-failure' },
  webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: true },
});
