import { test, expect, type Page } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { desktop } from './desktop-fixture';
import type { Snapshot } from '../../src/bindings/Snapshot';

async function routedProcesses(page: Page) {
  await desktop(page);
  await page.evaluate(() => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.config.revision += 1;
    snapshot.config.auto_rules = ['开发工具', '网页浏览'].map((name, index) => ({
      id: `rule-${index}`, name, enabled: true, proxy_group_id: 0, process_name: '*.exe', image_path_pattern: '', cmdline_pattern: '', protocol: 'both',
      dst_filter: { include_cidrs: [], exclude_cidrs: [], include_ports: [], exclude_ports: [] },
    }));
    snapshot.processes[0].route = { source: 'rule', rule_id: 'rule-0', group_id: 0 };
    snapshot.processes[1].route = { source: 'rule', rule_id: 'rule-1', group_id: 0 };
  });
  await page.getByRole('button', { name: '立即刷新' }).click();
}

test('processes using the same proxy group show the rule selected by the backend', async ({ page }) => {
  await routedProcesses(page);
  await expect(page.getByRole('columnheader', { name: '代理来源' })).toBeVisible();
  const code = page.getByRole('row').filter({ hasText: 'Code.exe' });
  const chrome = page.getByRole('row').filter({ hasText: 'chrome.exe' });
  await expect(code).toContainText('开发工具');
  await expect(chrome).toContainText('网页浏览');
  await expect(code).toContainText('本地代理'); await expect(chrome).toContainText('本地代理');
  await expect(page.getByRole('row').filter({ hasText: 'pwsh.exe' })).toContainText('直连');
  await page.getByRole('radio', { name: '已选代理', exact: true }).check();
  await expect(code).toBeVisible(); await expect(chrome).toBeVisible();
  await expect(page.getByText('pwsh.exe', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-rule-attribution.png') });
  await code.getByRole('button', { name: '详情与代理' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('当前代理来源：开发工具', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: '应用到进程树' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(code).toContainText('手动选择（含继承）');
  await expect(code).not.toContainText('开发工具');
});

test('an open detail follows rule renames and route changes without mixing reused PIDs', async ({ page }) => {
  await routedProcesses(page);
  await page.getByRole('row').filter({ hasText: 'Code.exe' }).getByRole('button', { name: '详情与代理' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('当前代理来源：开发工具', { exact: true })).toBeVisible();
  await page.evaluate(() => { const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot; snapshot.config.revision += 1; snapshot.config.auto_rules[0].name = '开发规则已重命名'; });
  await expect(dialog.getByText('当前代理来源：开发规则已重命名', { exact: true })).toBeVisible();
  await page.evaluate(() => { (window as unknown as { snapshot: Snapshot }).snapshot.processes[0].route = { group_id: 0, rule_id: null, source: 'manual' }; });
  await expect(dialog.getByText('当前代理来源：手动选择（含继承）', { exact: true })).toBeVisible();
  await page.evaluate(() => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.processes[0].process.identity.sequence = 'new-process';
    snapshot.processes[0].route = { group_id: 0, rule_id: 'rule-1', source: 'rule' };
  });
  await expect(dialog.getByText('当前代理来源：进程已结束', { exact: true })).toBeVisible();
  await expect(dialog.getByText('当前代理来源：网页浏览', { exact: true })).toHaveCount(0);
});

test('missing rule data stays explicit and long rule names remain readable', async ({ page }) => {
  await routedProcesses(page);
  const longName = '用于工作环境的浏览器和后代进程网络连接的完整自动代理规则名称'.repeat(3);
  await page.evaluate(name => {
    const snapshot = (window as unknown as { snapshot: Snapshot }).snapshot;
    snapshot.config.revision += 1;
    snapshot.config.auto_rules[1].name = name;
    snapshot.processes[0].route = { group_id: 0, rule_id: 'missing-rule', source: 'rule' };
  }, longName);
  await page.getByRole('button', { name: '立即刷新' }).click();
  await page.setViewportSize({ width: 760, height: 560 });
  await expect(page.getByRole('row').filter({ hasText: 'Code.exe' })).toContainText('规则已不可用');
  const name = page.getByRole('row').filter({ hasText: 'chrome.exe' }).getByText(longName, { exact: true });
  await expect(name).toBeVisible(); await name.hover();
  await expect(page.getByRole('tooltip')).toContainText(longName);
});
