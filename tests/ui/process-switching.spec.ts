import { test, expect } from '@playwright/test';
import { desktop } from './desktop-fixture';
import type { Snapshot } from '../../src/bindings/Snapshot';

test('process and connection switches retain bounded tables and reuse loaded icons', async ({ page }) => {
  await desktop(page, { processCount: 300, customTitlebar: true });
  await page.evaluate(() => {
    const fixture = window as unknown as { snapshot: Snapshot; processTable?: Element; connectionTable?: Element };
    fixture.snapshot.connections = Array.from({ length: 300 }, (_, index) => ({ ...fixture.snapshot.connections[0], pid: 100 + index, local_port: 50000 + index }));
  });
  await page.getByRole('button', { name: '立即刷新', exact: true }).click();
  await expect(page.getByRole('table').locator('tbody tr')).toHaveCount(50);
  await expect(page.getByRole('table').locator('svg image').first()).toBeVisible();
  const calls = await page.evaluate(() => {
    const fixture = window as unknown as { iconCalls: unknown[]; processTable?: Element };
    fixture.processTable = document.querySelector('table')!;
    return fixture.iconCalls.length;
  });
  for (let index = 0; index < 3; index++) {
    await page.getByRole('button', { name: '连接', exact: true }).click();
    await expect(page.getByRole('columnheader', { name: '协议', exact: true })).toBeVisible();
    await expect(page.getByRole('table').locator('tbody tr')).toHaveCount(50);
    expect(await page.getByRole('table').evaluate(element => {
      const fixture = window as unknown as { connectionTable?: Element };
      fixture.connectionTable ??= element;
      return fixture.connectionTable === element;
    })).toBe(true);
    await page.getByRole('button', { name: '进程', exact: true }).click();
    await expect(page.getByRole('columnheader', { name: '应用', exact: true })).toBeVisible();
    expect(await page.getByRole('table').evaluate(element => element === (window as unknown as { processTable: Element }).processTable)).toBe(true);
  }
  expect(await page.evaluate(() => (window as unknown as { iconCalls: unknown[] }).iconCalls.length)).toBe(calls);
});

test('hidden process data stays current and returning tabs reset pagination', async ({ page }) => {
  await desktop(page, { processCount: 62 });
  await page.getByRole('button', { name: '转到下一页', exact: true }).click();
  await expect(page.getByRole('row').nth(1)).toHaveAttribute('aria-rowindex', '51');
  await page.getByRole('button', { name: '连接', exact: true }).click();
  await page.evaluate(() => {
    const fixture = window as unknown as { snapshot: Snapshot };
    fixture.snapshot.processes[0].process.name = 'updated-process.exe';
    fixture.snapshot.processes[0].route = { group_id: 0, rule_id: null, source: 'manual' };
  });
  await page.getByRole('button', { name: '立即刷新', exact: true }).click();
  await page.getByRole('button', { name: '进程', exact: true }).click();
  await expect(page.getByRole('row').nth(1)).toHaveAttribute('aria-rowindex', '1');
  await expect(page.getByRole('row').nth(1)).toContainText('updated-process.exe');
  await expect(page.getByRole('row').nth(1)).toContainText('本地代理');
});
