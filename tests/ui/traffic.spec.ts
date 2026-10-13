import { test, expect, type Page } from '@playwright/test';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { desktop } from './desktop-fixture';
import type { Snapshot } from '../../src/bindings/Snapshot';

async function trafficFixture(page: Page, processCount = 3) {
  await desktop(page, { processCount });
  await page.evaluate(() => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.config.proxy_groups.push({ id: 1, name: '工作代理', host: '127.0.0.1', port: 7891 });
    snapshot.processes.forEach((item, index) => { item.route = index % 3 === 2 ? null : { group_id: index % 3, rule_id: null, source: 'manual' }; });
    const stats = (uploaded: number, downloaded: number, up: number, down: number) => ({ uploaded_bytes: uploaded, downloaded_bytes: downloaded, upload_bytes_per_second: up, download_bytes_per_second: down });
    snapshot.traffic = {
      total: stats(1048576, 8388608, 3072, 8192),
      groups: [{ group_id: 0, stats: stats(262144, 2097152, 1024, 2048) }, { group_id: 1, stats: stats(786432, 6291456, 2048, 6144) }],
      processes: snapshot.processes.slice(0, 2).map((item, index) => ({ identity: item.process.identity, total: stats(32768, 65536, 256, 512), groups: [{ group_id: index, stats: stats(16384, 32768, 128, 256) }] })),
    };
  });
  await page.getByRole('button', { name: '立即刷新' }).click();
}
async function chooseGroup(page: Page, name: string) {
  await page.getByRole('button', { name: '筛选代理组', exact: true }).click();
  await page.getByRole('option', { name, exact: true }).click();
}

test('group, scope and search combine, reset pagination, and recover from deleted groups', async ({ page }) => {
  await trafficFixture(page, 120);
  await page.getByRole('button', { name: '转到下一页' }).click();
  await expect(page.getByRole('row').nth(1)).toHaveAttribute('aria-rowindex', '51');
  await chooseGroup(page, '工作代理');
  await expect(page.getByRole('row').nth(1)).toHaveAttribute('aria-rowindex', '1');
  await expect(page.getByText('共 40 项 · 每秒更新', { exact: true })).toBeVisible();
  await expect(page.getByText('已下载 6.0 MiB', { exact: true })).toBeVisible();
  await page.getByRole('radio', { name: '已选代理', exact: true }).check();
  const search = page.getByRole('textbox', { name: '搜索进程、PID 或地址' });
  await search.fill('101');
  const row = page.getByRole('row').filter({ hasText: 'PID 101' });
  await expect(row).toContainText('128 B/s');
  await expect(page.getByText('共 1 项 · 每秒更新', { exact: true })).toBeVisible();
  await search.fill('pwsh.exe'); await expect(page.getByText('没有匹配的进程', { exact: true })).toBeVisible();
  await search.fill('');
  await page.evaluate(() => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.config.proxy_groups = snapshot.config.proxy_groups.filter(group => group.id !== 1);
    snapshot.processes.forEach(item => { if (item.route?.group_id === 1) item.route.group_id = 0; });
  });
  await expect(page.getByRole('button', { name: '筛选代理组', exact: true })).toContainText('全部代理组');
  await page.evaluate(() => { (window as unknown as { snapshot: Snapshot }).snapshot.config.proxy_groups.push({ id: 1, name: '新代理', host: '127.0.0.1', port: 7891 }); });
  await page.getByRole('button', { name: '立即刷新' }).click();
  await expect(page.getByRole('button', { name: '筛选代理组', exact: true })).toContainText('全部代理组');
  await page.getByRole('button', { name: '连接', exact: true }).click();
  await expect(page.getByRole('button', { name: '筛选代理组', exact: true })).toHaveCount(0);
});

test('live backend rates, totals and details stay usable in a small window and reset on pause', async ({ page }) => {
  await trafficFixture(page);
  const row = page.getByRole('row').filter({ hasText: 'Code.exe' });
  await expect(row).toContainText('256 B/s');
  await expect(page.getByText('已下载 8.0 MiB', { exact: true })).toBeVisible();
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-traffic-1280.png') });
  await page.setViewportSize({ width: 760, height: 560 });
  await chooseGroup(page, '本地代理');
  await expect(row).toContainText('128 B/s');
  await expect(page.getByRole('columnheader', { name: '实时流量' })).toBeInViewport({ ratio: 1 });
  await row.getByRole('button', { name: '详情与代理' }).click({ trial: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy();
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-traffic-760.png') });
  await row.getByRole('button', { name: '详情与代理' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('已上传 32.0 KiB', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: '应用到进程树' })).toBeInViewport({ ratio: 1 });
  await page.evaluate(() => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.traffic.processes[0].total.download_bytes_per_second = 4096;
    snapshot.traffic.processes[0].total.downloaded_bytes = 1048576;
  });
  await expect(dialog.getByText('4.0 KiB/s', { exact: true })).toBeVisible();
  await expect(dialog.getByText('已下载 1.0 MiB', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: '取消', exact: true }).click();
  await page.evaluate(() => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.stats.running = false;
    snapshot.traffic = { total: { uploaded_bytes: 0, downloaded_bytes: 0, upload_bytes_per_second: 0, download_bytes_per_second: 0 }, groups: [], processes: [] };
  });
  await expect(row.getByText('0 B/s', { exact: true })).toHaveCount(2);
  await expect(page.getByText('已下载 0 B', { exact: true })).toBeVisible();
});
