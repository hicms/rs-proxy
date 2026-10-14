import { test, expect } from '@playwright/test';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { desktop } from './desktop-fixture';

test('caption controls reflect native state and preserve navigation and engine actions', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await desktop(page, { customTitlebar: true });
  const caption = page.getByRole('navigation', { name: '主导航', exact: true });
  await expect(caption).toHaveAttribute('data-tauri-drag-region', 'deep');
  await expect(page.getByRole('button', { name: '最小化窗口', exact: true })).toBeVisible();
  expect((await page.getByRole('table').boundingBox())!.y).toBeLessThan(180);
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-titlebar-light.png') });
  await page.getByRole('button', { name: '最大化窗口', exact: true }).click();
  await expect(page.getByRole('button', { name: '还原窗口', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '还原窗口', exact: true }).click();
  await expect(page.getByRole('button', { name: '最大化窗口', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '最小化窗口', exact: true }).click();
  await page.getByRole('button', { name: '暂停代理', exact: true }).click();
  await expect(page.getByRole('button', { name: '启动代理', exact: true })).toBeVisible();
  await page.getByRole('link', { name: '路由规则', exact: true }).click();
  await expect(page.getByRole('heading', { name: '路由规则', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '关闭窗口', exact: true }).click();
  expect(await page.evaluate(() => (window as unknown as { windowCalls: string[] }).windowCalls)).toEqual([
    'plugin:window|toggle_maximize', 'plugin:window|toggle_maximize', 'plugin:window|minimize', 'plugin:window|close',
  ]);
  expect(errors).toEqual([]);
});

test('window controls stay actionable above an open modal and return to the caption', async ({ page }) => {
  await desktop(page, { customTitlebar: true });
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await page.getByRole('button', { name: '新建代理组', exact: true }).click();
  const dialog = page.getByRole('dialog');
  const controls = dialog.locator('.window-controls');
  await expect(controls).toBeVisible();
  await expect(controls).toHaveJSProperty('popover', 'manual');
  const bounds = await controls.boundingBox();
  expect(bounds!.y).toBeLessThan(48);
  await controls.getByRole('button', { name: '最大化窗口', exact: true }).click();
  await expect(controls.getByRole('button', { name: '还原窗口', exact: true })).toBeVisible();
  await controls.getByRole('button', { name: '最小化窗口', exact: true }).click();
  await controls.getByRole('button', { name: '关闭窗口', exact: true }).click();
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('navigation', { name: '主导航', exact: true }).getByRole('button', { name: '还原窗口', exact: true })).toBeVisible();
});

test('caption adapts to dark theme and minimum window width without overflow', async ({ page }) => {
  await desktop(page, { customTitlebar: true });
  await page.getByRole('link', { name: '设置', exact: true }).click();
  await page.getByRole('combobox', { name: '主题' }).click();
  await page.getByRole('option', { name: '深色', exact: true }).click();
  await page.getByRole('button', { name: '保存设置', exact: true }).click();
  await expect(page.getByRole('combobox', { name: '主题' })).toContainText('深色');
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-titlebar-dark.png') });
  await page.setViewportSize({ width: 760, height: 560 });
  await expect(page.getByRole('button', { name: '关闭窗口', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy();
  const caption = page.getByRole('navigation', { name: '主导航', exact: true });
  const bounds = await caption.boundingBox();
  expect(bounds!.height).toBe(48);
  expect(bounds!.y).toBe(0);
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-titlebar-narrow.png') });
  await page.getByRole('button', { name: '打开导航', exact: true }).click();
  await expect(page.locator('dialog:modal .window-controls')).toHaveCount(1);
  await page.getByRole('button', { name: '最大化窗口', exact: true }).click();
  await expect(page.getByRole('button', { name: '还原窗口', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '关闭导航', exact: true }).click();
  await expect(page.locator('dialog:modal')).toHaveCount(0);
});

test('native operation failure remains visible without moving the caption', async ({ page }) => {
  await desktop(page, { customTitlebar: true });
  await page.evaluate(() => { (window as unknown as { windowError: string }).windowError = '窗口操作失败'; });
  await page.getByRole('button', { name: '最大化窗口', exact: true }).click();
  await expect(page.getByText('窗口操作失败', { exact: true })).toBeVisible();
  const bounds = await page.getByRole('navigation', { name: '主导航', exact: true }).boundingBox();
  expect(bounds!.y).toBe(0);
  await expect(page.getByRole('button', { name: '最大化窗口', exact: true })).toBeVisible();
});
