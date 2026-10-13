import { chromium, expect } from '@playwright/test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { focusExistingInstance } from './second-instance.mjs';
import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { writeFile } from 'node:fs/promises';

const windows = (action, value = '') => execFileSync(`${process.env.SystemRoot}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe`,
  ['-NoProfile', '-NonInteractive', '-File', resolve('tests/native/windows.ps1'), '-Action', action, '-ProcessId', process.env.RS_PROXY_E2E_PID, '-Value', value],
  { encoding: 'utf8', windowsHide: true, timeout: 15000 });
let browser;
try {
  await expect.poll(async () => {
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9225'); return true; } catch { return false; }
  }, { timeout: 20000 }).toBeTruthy();
  const page = browser.contexts()[0].pages()[0];
  await expect(page.getByRole('heading', { name: '进程与连接', exact: true })).toBeVisible();
  const config = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('get_snapshot').then(value => value.config));
  assert.equal(config.dns.enabled, false); assert.equal(config.ui.theme, 'light');
  assert.equal(config.tcp_syn_parking.watchdog_ms, 25);
  assert.equal(config.proxy_groups.some(group => group.name === '验证 B'), true);
  assert.equal(config.auto_rules.length, 0);
  assert.ok(JSON.parse(windows('inspect')).some(window => window.Title === 'RS Proxy E2E' && !window.Visible));
  await focusExistingInstance();
  await expect.poll(() => JSON.parse(windows('inspect')).some(window => window.Title === 'RS Proxy E2E' && window.Visible)).toBeTruthy();
  await page.getByRole('link', { name: '设置', exact: true }).click();
  await expect(page.getByRole('combobox', { name: '主题' })).toContainText('浅色');
  await page.getByRole('checkbox', { name: '关闭窗口时收起到托盘' }).uncheck();
  await page.getByRole('button', { name: '保存设置', exact: true }).click();
  await expect(page.getByText('设置已保存', { exact: true })).toBeVisible();
  windows('close', 'RS Proxy E2E');
  await expect.poll(() => { try { process.kill(Number(process.env.RS_PROXY_E2E_PID), 0); return false; } catch { return true; } }, { timeout: 15000 }).toBeTruthy();
  await writeFile(join(tmpdir(), 'rs-proxy-e2e', 'restart.json'), JSON.stringify({ persistence: true, minimizedStartup: true, closeExits: true }));
  console.log('PASS restart persistence, minimized startup, close without tray exits');
} finally { await browser?.close(); }
