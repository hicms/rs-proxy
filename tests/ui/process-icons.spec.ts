import { test, expect } from '@playwright/test';
import { desktop } from './desktop-fixture';

test('program images appear only after loading; missing and invalid images fall back', async ({ page }) => {
  await desktop(page, { iconDelay: 1500 });
  const rows = page.getByRole('row');
  await expect(rows.locator('.lucide-app-window')).toHaveCount(0);
  await expect(rows.locator('svg image')).toHaveCount(0);
  await expect(rows.nth(1).locator('svg image')).toHaveCount(1);
  await expect(rows.nth(2).locator('svg image')).toHaveCount(1);
  await expect(rows.nth(3).locator('.lucide-app-window')).toHaveCount(1);
  const urls = await rows.locator('svg image').evaluateAll(images => images.map(image => image.getAttribute('href')));
  expect(new Set(urls).size).toBe(2);
  await expect.poll(() => page.evaluate(async () => {
    const urls = Array.from(document.querySelectorAll('svg image'), image => image.getAttribute('href')!);
    return Promise.all(urls.map(url => new Promise<boolean>(resolve => { const image = new Image(); image.onload = () => resolve(image.naturalWidth > 0); image.onerror = () => resolve(false); image.src = url; })));
  })).toEqual([true, true]);
  await page.getByRole('button', { name: '立即刷新' }).click();
  expect(await page.evaluate(() => (window as unknown as { iconCalls: unknown[] }).iconCalls.length)).toBe(1);
  await rows.nth(1).locator('svg image').dispatchEvent('error');
  await expect(rows.nth(1).locator('.lucide-app-window')).toHaveCount(1);
});

test('broken PNG and IPC failure use the fallback', async ({ page }) => {
  await desktop(page, { brokenIcon: true });
  await expect(page.getByRole('row').locator('.lucide-app-window')).toHaveCount(3);
  await page.evaluate(() => { (window as unknown as Record<string, unknown>).iconError = true; });
  await page.getByRole('textbox', { name: '搜索进程、PID 或地址' }).fill('chrome');
  await expect(page.getByRole('row').nth(1).locator('.lucide-app-window')).toHaveCount(1);
  await expect(page.getByRole('row').nth(1).locator('svg image')).toHaveCount(0);
});

test('visible pages are bounded and obsolete requests are coalesced across navigation', async ({ page }) => {
  await desktop(page, { processCount: 62, iconDelay: 1800 });
  await page.getByRole('button', { name: '转到下一页' }).click();
  await page.getByRole('link', { name: '设置', exact: true }).click();
  await page.getByRole('link', { name: '进程与连接', exact: true }).click();
  const search = page.getByRole('textbox', { name: '搜索进程、PID 或地址' });
  await search.fill('151');
  await search.fill('153');
  await expect(page.getByRole('row').nth(1)).toContainText('PID 153');
  await expect(page.getByRole('row').nth(1).locator('.lucide-app-window')).toHaveCount(1);
  await expect(page.getByRole('row').nth(1).locator('svg image')).toHaveCount(0);
  const calls = await page.evaluate(() => {
    const scope = window as unknown as { iconCalls: { pid: number }[][]; iconMaxActive: number };
    return { calls: scope.iconCalls, max: scope.iconMaxActive };
  });
  expect(calls.max).toBe(1);
  expect(calls.calls[0]).toHaveLength(50);
  expect(calls.calls.every(batch => batch.length <= 50)).toBeTruthy();
  expect(calls.calls.at(-1)?.map(identity => identity.pid)).toEqual([153]);
  await search.fill('');
  await page.getByRole('button', { name: '转到下一页' }).click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { iconCalls: unknown[][] }).iconCalls.at(-1)?.length)).toBe(12);
  await expect(page.getByRole('row').locator('svg image').first()).toBeVisible();
});
