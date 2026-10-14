import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium, expect } from '@playwright/test';

const processId = Number(process.env.RS_PROXY_TITLEBAR_PID);
assert.ok(processId, 'Standalone harness must supply its owned application PID');
const nativeAction = action => execFileSync(
  `${process.env.SystemRoot}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe`,
  ['-NoProfile', '-NonInteractive', '-File', resolve('tests/native/windows.ps1'), '-Action', action, '-ProcessId', String(processId)],
  { encoding: 'utf8', windowsHide: true, timeout: 15000 },
);
const browser = await chromium.connectOverCDP('http://127.0.0.1:9226');
const page = browser.contexts()[0].pages()[0];
assert.ok(page, 'Standalone WebView2 page missing');
page.setDefaultTimeout(12000);
const errors = [];
page.on('pageerror', error => errors.push(error.message));
page.on('console', message => { if (['error', 'warning'].includes(message.type())) errors.push(message.text()); });
const rpc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const state = name => rpc(`plugin:window|${name}`, { label: 'main' });
const button = name => page.getByRole('button', { name, exact: true });
const originalConfig = (await rpc('get_snapshot')).config;
const saveUi = async patch => {
  const config = (await rpc('get_snapshot')).config;
  await rpc('save_config', { config: { ...config, ui: { ...config.ui, ...patch } } });
  await expect.poll(async () => (await rpc('get_snapshot')).config.ui).toMatchObject(patch);
};

try {
  assert.match(page.url(), /^http:\/\/tauri\.localhost\//);
  assert.equal(await page.title(), 'RS Proxy');
  await expect(page.getByRole('heading', { name: '进程与连接', exact: true })).toBeVisible();
  assert.equal(await state('is_decorated'), false);
  await expect(button('关闭窗口')).toBeVisible();
  nativeAction('restore');
  await expect.poll(() => state('is_visible')).toBe(true);
  console.log(`PASS native caption drag: ${nativeAction('drag').trim()}`);

  await button('最大化窗口').click();
  await expect.poll(() => state('is_maximized')).toBe(true);
  await expect(button('还原窗口')).toBeVisible();
  await button('还原窗口').click();
  await expect.poll(() => state('is_maximized')).toBe(false);
  // Dispatch the second click through the installed native drag-region handler.
  await page.locator('.window-titlebar').dispatchEvent('mousedown', { button: 0, detail: 2 });
  await expect.poll(() => state('is_maximized')).toBe(true);
  await expect(button('还原窗口')).toBeVisible();
  await page.locator('.window-titlebar').dispatchEvent('mousedown', { button: 0, detail: 2 });
  await expect.poll(() => state('is_maximized')).toBe(false);
  console.log('PASS native maximize, restore and caption double-click state synchronization');

  await saveUi({ theme: 'light' });
  await expect.poll(() => page.locator('.rs-proxy-shell').evaluate(element => getComputedStyle(element).colorScheme)).toBe('light');
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-native-titlebar-light.png') });
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await button('新建代理组').click();
  await expect(page.locator('dialog:modal .window-controls')).toHaveCount(1);
  await button('最大化窗口').click();
  await expect.poll(() => state('is_maximized')).toBe(true);
  await button('还原窗口').click();
  await expect.poll(() => state('is_maximized')).toBe(false);
  await button('最小化窗口').click();
  await expect.poll(() => state('is_minimized')).toBe(true);
  nativeAction('restore');
  await expect.poll(() => state('is_minimized')).toBe(false);
  await page.getByRole('dialog').getByRole('button', { name: '取消', exact: true }).click();
  await expect(page.locator('dialog:modal')).toHaveCount(0);
  console.log('PASS native window controls while a modal dialog is open');

  await saveUi({ theme: 'dark' });
  await expect.poll(() => page.locator('.rs-proxy-shell').evaluate(element => getComputedStyle(element).colorScheme)).toBe('dark');
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-native-titlebar-dark.png') });
  await page.setViewportSize({ width: 760, height: 560 });
  await expect(button('关闭窗口')).toBeVisible();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await button('打开导航').click();
  await expect(page.locator('dialog:modal .window-controls')).toHaveCount(1);
  await button('最小化窗口').click();
  await expect.poll(() => state('is_minimized')).toBe(true);
  nativeAction('restore');
  await expect.poll(() => state('is_minimized')).toBe(false);
  await button('关闭导航').click();
  await expect(page.locator('dialog:modal')).toHaveCount(0);
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-native-titlebar-narrow.png') });
  await page.setViewportSize({ width: 1280, height: 820 });
  console.log('PASS native light/dark caption and minimum-width modal navigation');

  await saveUi({ close_to_tray: true });
  await button('关闭窗口').click();
  await expect.poll(() => state('is_visible')).toBe(false);
  nativeAction('restore');
  await expect.poll(() => state('is_visible')).toBe(true);
  await saveUi({ ...originalConfig.ui, close_to_tray: false });
  assert.deepEqual(errors, []);
  await button('关闭窗口').click();
  console.log('PASS close to tray and caption-triggered orderly exit; no runtime errors');
} finally {
  await browser.close();
}
