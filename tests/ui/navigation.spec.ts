import { test, expect } from '@playwright/test';
import { desktop } from './desktop-fixture';
import type { Snapshot } from '../../src/bindings/Snapshot';

type Fixture = { snapshot: Snapshot; iconCalls: unknown[]; autostartReads: number; table?: Element };

test('sidebar starts expanded, collapses, and leaves room around page content', async ({ page }) => {
  await desktop(page, { customTitlebar: true });
  const nav = page.getByRole('navigation', { name: '主导航', exact: true });
  await expect(nav.getByRole('link', { name: '设置', exact: true })).toBeVisible();
  const expanded = (await nav.boundingBox())!;
  expect(expanded.width).toBeGreaterThan(160);
  expect(expanded.width).toBeLessThan(241);
  const heading = (await page.getByRole('heading', { name: '进程与连接', exact: true }).boundingBox())!;
  const table = (await page.getByRole('table').boundingBox())!;
  expect(heading.x - expanded.x - expanded.width).toBeGreaterThanOrEqual(24);
  expect(1280 - table.x - table.width).toBeGreaterThanOrEqual(24);
  await nav.getByRole('button', { name: /Collapse|收起|折叠/ }).click();
  await expect.poll(async () => (await nav.boundingBox())!.width).toBeLessThan(100);
  await nav.getByRole('link', { name: '设置', exact: true }).click();
  await expect(page.getByRole('heading', { name: '设置', exact: true })).toBeVisible();
});

test('visited pages retain table, filter, pagination and icons while consuming fresh snapshots', async ({ page }) => {
  await desktop(page, { processCount: 180 });
  await page.getByRole('textbox', { name: '搜索进程、PID 或地址' }).fill('chrome');
  await page.getByRole('button', { name: '转到下一页' }).click();
  await expect(page.getByRole('row').nth(1).locator('svg image')).toBeVisible();
  const calls = await page.getByRole('table').evaluate(element => {
    const scope = window as unknown as Fixture; scope.table = element; return scope.iconCalls.length;
  });
  expect(await page.evaluate(() => (window as unknown as Fixture).autostartReads)).toBe(0);
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await page.evaluate(() => {
    const scope = window as unknown as Fixture;
    scope.snapshot.processes[151].route = { group_id: 0, source: 'manual', rule_id: null };
  });
  await expect.poll(() => page.evaluate(() => (window as unknown as Fixture).autostartReads)).toBe(0);
  // Let the normal snapshot poll run while this table is hidden.
  await page.waitForTimeout(1200);
  await page.getByRole('link', { name: '进程与连接', exact: true }).click();
  await expect(page.getByRole('textbox', { name: '搜索进程、PID 或地址' })).toHaveValue('chrome');
  await expect(page.getByRole('row').nth(1)).toHaveAttribute('aria-rowindex', '51');
  await expect(page.getByRole('row').nth(1)).toContainText('手动选择（含继承）');
  expect(await page.getByRole('table').evaluate(element => element === (window as unknown as Fixture).table)).toBe(true);
  expect(await page.evaluate(() => (window as unknown as Fixture).iconCalls.length)).toBe(calls);
  await page.getByRole('link', { name: '设置', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as Fixture).autostartReads)).toBeGreaterThan(0);
});

test('hidden pages release their modal and retained editors preserve revision conflicts', async ({ page }) => {
  await desktop(page, { customTitlebar: true });
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await page.getByRole('button', { name: '编辑', exact: true }).click();
  await page.getByRole('textbox', { name: '名称 必填', exact: true }).fill('未保存名称');
  await page.evaluate(() => { location.hash = 'processes'; });
  await expect(page.locator('dialog:modal')).toHaveCount(0);
  await expect(page.getByRole('navigation', { name: 'RS Proxy 窗口' }).getByRole('button', { name: '关闭窗口' })).toBeVisible();
  await page.evaluate(() => { const scope = window as unknown as Fixture; scope.snapshot.config.revision++; });
  await page.getByRole('button', { name: '立即刷新' }).click();
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByRole('textbox', { name: '名称 必填', exact: true })).toHaveValue('未保存名称');
  await page.getByRole('dialog').getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByText('配置已被更新', { exact: true })).toBeVisible();
});

test('settings fade separators and proxy search work in the official themes', async ({ page }) => {
  await desktop(page);
  await page.getByRole('link', { name: '设置', exact: true }).click();
  const separators = page.locator('.astryx-divider[aria-orientation="vertical"]');
  await expect(separators).toHaveCount(3);
  expect(await separators.first().evaluate(element => getComputedStyle(element).maskImage)).toContain('linear-gradient');
  await page.getByRole('button', { name: '网络', exact: true }).click();
  const dnsHeading = (await page.getByRole('heading', { name: '进程 DNS', exact: true }).boundingBox())!;
  const upstream = (await page.getByRole('textbox', { name: '上游 DNS IPv4 地址' }).boundingBox())!;
  const divider = (await separators.first().boundingBox())!;
  expect(divider.x).toBeGreaterThan(dnsHeading.x + dnsHeading.width);
  expect(upstream.x).toBeGreaterThan(divider.x);
  expect(upstream.y - dnsHeading.y).toBeLessThan(180);
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await page.getByRole('textbox', { name: '搜索代理组' }).fill('7890');
  await expect(page.getByRole('row').nth(1)).toContainText('本地代理');
  await page.getByRole('textbox', { name: '搜索代理组' }).fill('missing');
  await expect(page.getByText('没有匹配的代理组')).toBeVisible();
  await page.getByRole('textbox', { name: '搜索代理组' }).fill('');
});

test('hidden process views suspend icon polling and refresh changed identities on return', async ({ page }) => {
  await desktop(page, { iconDelay: 200 });
  await expect(page.getByRole('row').nth(1).locator('svg image')).toBeVisible();
  await page.clock.install();
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  const calls = await page.evaluate(() => (window as unknown as Fixture).iconCalls.length);
  await page.clock.fastForward(35000);
  expect(await page.evaluate(() => (window as unknown as Fixture).iconCalls.length)).toBe(calls);
  await page.evaluate(() => {
    const scope = window as unknown as Fixture;
    scope.snapshot.processes[0].process.identity.sequence = 'replacement';
    scope.snapshot.processes[0].process.image_path = 'C:\\New\\Code.exe';
  });
  await page.clock.fastForward(1000);
  await page.getByRole('link', { name: '进程与连接', exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as Fixture).iconCalls.length)).toBe(calls + 1);
  await page.clock.fastForward(250);
  await expect(page.getByRole('row').nth(1).locator('svg image')).toBeVisible();
});
