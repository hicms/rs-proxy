import { test, expect, type Locator, type Page } from '@playwright/test';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { desktop } from './desktop-fixture';
import type { Snapshot } from '../../src/bindings/Snapshot';

async function navigate(page: Page, name: string) {
  const toggle = page.getByRole('button', { name: '打开导航', exact: true });
  if (await toggle.isVisible()) await toggle.click();
  await page.getByRole('link', { name, exact: true }).click();
  await expect(page.getByRole('heading', { name, exact: true })).toBeVisible();
}

async function reachable(dialog: Locator, label = '保存') {
  const button = dialog.getByRole('button', { name: label, exact: true });
  await expect(button).toBeInViewport({ ratio: 1 });
  // Viewport intersection alone misses clipping by the modal's inner container.
  await expect.poll(() => button.evaluate(element => {
    const rect = element.getBoundingClientRect();
    const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
    return hit !== null && element.contains(hit);
  })).toBeTruthy();
  await button.click({ trial: true, timeout: 2000 });
}

test('other editors and confirmations remain usable at the minimum window size', async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 560 });
  await desktop(page);
  const dialog = page.getByRole('dialog');
  await navigate(page, '代理组');
  await page.getByRole('button', { name: '新建代理组' }).click();
  await reachable(dialog);
  await dialog.getByRole('textbox', { name: '名称 必填', exact: true }).fill('备用代理');
  // Keyboard commit must stay reachable as well as the mouse action.
  await dialog.getByRole('button', { name: '取消', exact: true }).focus();
  await page.keyboard.press('Tab');
  await expect(dialog.getByRole('button', { name: '保存', exact: true })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(dialog).toHaveCount(0);
  await page.getByRole('row').filter({ hasText: '备用代理' }).getByRole('button', { name: '编辑', exact: true }).click();
  await reachable(dialog);
  await dialog.getByRole('textbox', { name: '名称 必填', exact: true }).fill('备用代理已编辑');
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(dialog).toHaveCount(0);

  await page.evaluate(() => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.connections = Array.from({ length: 40 }, (_, index) => ({ ...snapshot.connections[0], pid: 100, local_port: 51000 + index }));
  });
  await navigate(page, '进程与连接');
  await page.getByRole('button', { name: '立即刷新' }).click();
  await page.getByRole('row').filter({ hasText: 'Code.exe' }).getByRole('button', { name: '详情与代理' }).click();
  await reachable(dialog, '应用到进程树');
  await dialog.getByRole('combobox', { name: '代理组', exact: true }).click();
  await page.getByRole('option', { name: '备用代理已编辑', exact: true }).click();
  const body = dialog.locator('.astryx-layout-content');
  await body.hover(); await page.mouse.wheel(0, 4000);
  await expect.poll(() => body.evaluate(element => element.scrollTop)).toBeGreaterThan(0);
  await reachable(dialog, '应用到进程树');
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-process-dialog-760.png') });
  await dialog.getByRole('button', { name: '应用到进程树', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('row').filter({ hasText: 'Code.exe' })).toContainText('备用代理已编辑');

  await navigate(page, '代理组');
  await page.getByRole('row').filter({ hasText: '备用代理已编辑' }).getByRole('button', { name: '删除', exact: true }).click();
  await reachable(dialog, '替换引用并删除');
  expect((await dialog.boundingBox())!.height).toBeLessThan(560 * 0.75);
  await dialog.getByRole('button', { name: '替换引用并删除', exact: true }).click();
  await expect(dialog).toHaveCount(0);

  await openRules(page);
  await dialog.getByRole('textbox', { name: '规则名称' }).fill('待删除规则');
  await dialog.getByRole('textbox', { name: '进程名称', exact: true }).fill('chrome.exe');
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page.getByRole('row').filter({ hasText: '待删除规则' }).getByRole('button', { name: '删除', exact: true }).click();
  await reachable(dialog, '删除规则');
  expect((await dialog.boundingBox())!.height).toBeLessThan(560 * 0.75);
  await dialog.getByRole('button', { name: '删除规则', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText('待删除规则', { exact: true })).toHaveCount(0);
});

async function openRules(page: Page) {
  await navigate(page, '路由规则');
  await page.getByRole('button', { name: '新建规则' }).click();
  return page.getByRole('dialog');
}

for (const viewport of [{ width: 1280, height: 720 }, { width: 760, height: 560 }]) {
  test(`rule dialog keeps actions reachable with a scrolling body at ${viewport.width}x${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await desktop(page);
    const dialog = await openRules(page);
    await reachable(dialog);
    await dialog.getByRole('button', { name: '保存', exact: true }).click();
    await expect(dialog.getByText('操作未完成', { exact: true })).toBeVisible();
    await reachable(dialog);
    await dialog.getByRole('textbox', { name: '规则名称' }).fill('普通窗口规则');
    await dialog.getByRole('textbox', { name: '进程名称', exact: true }).fill('chrome.exe');
    const body = dialog.locator('.astryx-layout-content');
    await expect.poll(() => body.evaluate(element => element.scrollHeight > element.clientHeight && ['auto', 'scroll'].includes(getComputedStyle(element).overflowY))).toBeTruthy();
    const footer = dialog.locator('.astryx-layout-footer');
    const before = await footer.boundingBox();
    await body.hover(); await page.mouse.wheel(0, 1600);
    await expect.poll(() => body.evaluate(element => element.scrollTop)).toBeGreaterThan(0);
    await expect(dialog.getByRole('checkbox', { name: '启用这条规则' })).toBeInViewport({ ratio: 1 });
    expect((await footer.boundingBox())?.y).toBeCloseTo(before!.y, 0);
    await reachable(dialog); await reachable(dialog, '取消');
    await page.screenshot({ path: join(tmpdir(), `rs-proxy-rule-dialog-${viewport.width}.png`) });
    await dialog.getByRole('button', { name: '保存', exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await page.getByRole('row').filter({ hasText: '普通窗口规则' }).getByRole('button', { name: '编辑', exact: true }).click();
    await expect(dialog.getByRole('heading', { name: '编辑路由规则' })).toBeVisible();
    await reachable(dialog);
    await page.setViewportSize({ width: 960, height: 600 });
    await reachable(dialog);
    await dialog.getByRole('button', { name: '取消', exact: true }).click();
    await expect(dialog).toHaveCount(0);
  });
}
