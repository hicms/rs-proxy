import { defineConfig } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
export default defineConfig({
  forbidOnly: !!process.env.CI,
  testDir: './tests/ui', fullyParallel: false, workers: 1,
  outputDir: join(tmpdir(), 'rs-proxy-playwright'),
  use: { baseURL: 'http://127.0.0.1:1420', viewport: { width: 1280, height: 820 }, channel: process.env.CI ? undefined : 'msedge', trace: 'retain-on-failure' },
  webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: !process.env.CI },
});
