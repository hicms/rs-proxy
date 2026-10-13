import { test, expect } from '@playwright/test';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { desktop } from './desktop-fixture';
import type { Snapshot } from '../../src/bindings/Snapshot';
import type { ProcessIdentity } from '../../src/bindings/ProcessIdentity';
import type { ReconnectResult } from '../../src/bindings/ReconnectResult';

type Fixture = { snapshot: Snapshot; reconnectCalls: { identities: ProcessIdentity[]; groupId: number | null }[]; reconnectDelay?: number; reconnectError?: string; reconnectResult?: ReconnectResult };

test('reconnect confirms filtered identities across pages, cancels without mutation and reports the result', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await desktop(page, { processCount: 180 });
  await expect(page.getByRole('button', { name: '重连已选代理', exact: true })).toBeDisabled();
  await page.evaluate(() => {
    const fixture = window as unknown as Fixture;
    fixture.snapshot.config.proxy_groups.push({ id: 1, name: '工作代理', host: '127.0.0.1', port: 7891 });
    fixture.snapshot.processes.forEach((item, index) => { item.route = index % 3 === 2 ? null : { group_id: index % 3, rule_id: null, source: 'manual' }; });
    fixture.reconnectDelay = 400;
  });
  await page.getByRole('button', { name: '立即刷新', exact: true }).click();
  await page.getByRole('button', { name: '筛选代理组', exact: true }).click();
  await page.getByRole('option', { name: '工作代理', exact: true }).click();
  await page.getByRole('textbox', { name: '搜索进程、PID 或地址' }).fill('chrome.exe');
  await page.getByRole('button', { name: '转到下一页' }).click();
  await page.getByRole('button', { name: '重连已选代理', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('60 个已选代理进程（全部分页）');
  await expect(dialog).toContainText('工作代理 · 搜索“chrome.exe”');
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as Fixture).reconnectCalls.length)).toBe(0);
  await page.getByRole('button', { name: '重连已选代理', exact: true }).click();
  await page.setViewportSize({ width: 760, height: 560 });
  await expect(dialog.getByRole('button', { name: '断开旧连接', exact: true })).toBeInViewport({ ratio: 1 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy();
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-reconnect-confirm-760.png'), animations: 'disabled' });
  await dialog.getByRole('button', { name: '断开旧连接', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '取消', exact: true })).toBeDisabled();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('已断开 2 条旧连接', { exact: true })).toBeVisible();
  const calls = await page.evaluate(() => (window as unknown as Fixture).reconnectCalls);
  expect(calls).toHaveLength(1); expect(calls[0].groupId).toBe(1);
  expect(calls[0].identities).toHaveLength(60);
  expect(calls[0].identities.every(id => id.pid % 3 === 2)).toBeTruthy();
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-reconnect-result-1280.png') });
  await page.getByRole('button', { name: '连接', exact: true }).click();
  await expect(page.getByRole('button', { name: '重连已选代理', exact: true })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('reconnect shows partial, zero and command errors and refuses a stopped engine', async ({ page }) => {
  await desktop(page);
  await page.evaluate(() => {
    const fixture = window as unknown as Fixture;
    fixture.snapshot.processes[0].route = { group_id: 0, rule_id: null, source: 'manual' };
    fixture.reconnectError = '代理状态已变化';
  });
  await page.getByRole('button', { name: '立即刷新', exact: true }).click();
  await page.getByRole('button', { name: '重连已选代理', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '断开旧连接', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('代理状态已变化');
  await page.evaluate(() => {
    const fixture = window as unknown as Fixture;
    fixture.reconnectError = undefined;
    fixture.reconnectResult = { disconnected: 1, skipped: 2, failures: ['PID 100：拒绝访问'] };
  });
  await page.getByRole('dialog').getByRole('button', { name: '断开旧连接', exact: true }).click();
  await expect(page.getByText('已断开 1 条旧连接', { exact: true })).toBeVisible();
  await expect(page.getByText(/^保留或已结束 2 条连接。1 项未能处理/)).toBeVisible();
  await page.evaluate(() => { (window as unknown as Fixture).reconnectResult = { disconnected: 0, skipped: 3, failures: [] }; });
  await page.getByRole('button', { name: '重连已选代理', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '断开旧连接', exact: true }).click();
  await expect(page.getByText('没有需要断开的旧连接', { exact: true })).toBeVisible();
  await page.evaluate(() => { (window as unknown as Fixture).snapshot.stats.running = false; });
  await page.getByRole('button', { name: '立即刷新', exact: true }).click();
  await expect(page.getByRole('button', { name: '重连已选代理', exact: true })).toBeDisabled();
});
