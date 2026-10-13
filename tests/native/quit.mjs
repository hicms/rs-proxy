import { chromium } from 'playwright';
try {
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9225');
  const page = browser.contexts()[0].pages()[0];
  if (page) await page.evaluate(() => { void window.__TAURI_INTERNALS__.invoke('quit_app'); });
  await browser.close();
} catch (error) { console.error(error.message); process.exitCode = 1; }
