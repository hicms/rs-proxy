import { test, expect, type Page } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { Snapshot } from '../../src/bindings/Snapshot';

const fixture: Snapshot = {
  config: { version: 1, revision: 0, proxy_groups: [{ id: 0, name: '本地代理', host: '127.0.0.1', port: 7890 }], auto_rules: [], default_exclude_cidrs: ['127.0.0.0/8', '10.0.0.0/8'], dns: { enabled: false, upstream_host: '8.8.8.8', upstream_port: 53, listen_host: '127.0.0.2', listen_port: 53, proxy_group_id: 0 }, ui: { theme: 'light', close_to_tray: true }, log_level: 'info', tcp_syn_parking: { watchdog_ms: 20, pool_size: 256 } },
  processes: ['Code.exe', 'chrome.exe', 'pwsh.exe'].map((name, index) => ({ process: { identity: { pid: 100 + index, sequence: String(index + 1) }, parent: null, name, image_path: `C:\\Apps\\${name}`, cmdline: `${name} --profile work` }, route: null })),
  connections: [{ pid: 101, protocol: 'tcp', local_address: '10.0.0.2', local_port: 50123, remote_address: '192.0.2.1', remote_port: 443, state: 'ESTABLISHED' }],
  stats: { running: true, elevated: true, dns_active: false, tcp_connections: 1, udp_sessions: 0, proxied_packets: 100, direct_packets: 10, uploaded_bytes: 1024, downloaded_bytes: 8192, parked_syns: 1, timed_out_syns: 0, last_error: null },
};
async function desktop(page: Page) {
  await page.addInitScript(snapshot => {
    const scope = window as unknown as Record<string, unknown>;
    scope.isTauri = true;
    scope.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    scope.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command: string, args: Record<string, unknown> = {}) => {
        if (command.startsWith('plugin:event|')) return 1;
        if (command === 'get_snapshot') return structuredClone(snapshot);
        if (command === 'save_config') {
          const config = args.config as typeof snapshot.config;
          if (config.auto_rules.some(rule => !rule.name || (!rule.process_name && !rule.image_path_pattern && !rule.cmdline_pattern))) throw '规则至少需要一个进程匹配条件';
          if (config.revision !== snapshot.config.revision) throw '配置已被更新';
          snapshot.config = { ...config, revision: config.revision + 1 }; return snapshot.config;
        }
        if (command === 'get_autostart' || command === 'set_autostart') return { enabled: false, start_minimized: false };
        if (command === 'set_engine_running') { snapshot.stats.running = Boolean(args.running); return; }
        if (command === 'set_process_proxy') {
          const id = args.identity as { pid: number; sequence: string };
          const process = snapshot.processes.find(item => item.process.identity.pid === id.pid && item.process.identity.sequence === id.sequence);
          if (!process) throw '进程身份已变化';
          process.route = args.groupId === null ? null : { group_id: Number(args.groupId), rule_id: null, source: 'manual' }; return;
        }
        if (command === 'test_proxy') throw 'SOCKS5 连接被拒绝';
        if (command === 'delete_group') {
          if (args.revision !== snapshot.config.revision) throw '配置已被更新';
          snapshot.config.proxy_groups = snapshot.config.proxy_groups.filter(group => group.id !== args.from);
          snapshot.config.dns.proxy_group_id = Number(args.to); snapshot.config.revision += 1;
          return snapshot.config;
        }
        throw new Error(`Unexpected command ${command}`);
      },
    };
  }, fixture);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '进程与连接', exact: true })).toBeVisible();
}

test('browser mode explains that a desktop service is required', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '无法读取桌面服务' })).toBeVisible();
  await expect(page.getByRole('button', { name: '启动引擎', exact: true })).toBeDisabled();
});

test('proxy CRUD and rule priority actions reach the saved configuration', async ({ page }) => {
  await desktop(page);
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await page.getByRole('button', { name: '新建代理组' }).click();
  await page.getByRole('dialog').getByRole('textbox', { name: '名称 必填', exact: true }).fill('备用代理');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  const backup = page.getByRole('row').filter({ hasText: '备用代理' });
  await backup.getByRole('button', { name: '编辑', exact: true }).click();
  await page.getByRole('dialog').getByRole('textbox', { name: '名称 必填', exact: true }).fill('远程代理');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await page.getByRole('row').filter({ hasText: '本地代理' }).getByRole('button', { name: '删除', exact: true }).click();
  await page.getByRole('button', { name: '替换引用并删除' }).click();
  await expect(page.getByText('本地代理', { exact: true })).toHaveCount(0);
  await expect(page.getByText('远程代理', { exact: true })).toBeVisible();
  await page.getByRole('link', { name: '路由规则', exact: true }).click();
  for (const name of ['第一条', '第二条']) {
    await page.getByRole('button', { name: '新建规则' }).click();
    await page.getByRole('textbox', { name: '规则名称' }).fill(name);
    await page.getByRole('textbox', { name: '进程名称', exact: true }).fill('*.exe');
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
  }
  await page.getByRole('row').filter({ hasText: '第二条' }).getByRole('button', { name: '上移' }).click();
  await expect(page.getByRole('row').nth(1)).toContainText('第二条');
  await page.getByRole('row').filter({ hasText: '第二条' }).getByRole('button', { name: '删除', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '删除规则', exact: true }).click();
  await expect(page.getByText('第二条', { exact: true })).toHaveCount(0);
  await expect(page.getByText('第一条', { exact: true })).toBeVisible();
});

test('process identity, rule editing, proxy errors, theme and narrow layout', async ({ page }) => {
  const errors: string[] = []; const externalFonts: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (request.url().includes('fonts.googleapis') || request.url().includes('fonts.gstatic')) externalFonts.push(request.url()); });
  await desktop(page);
  await page.getByRole('textbox', { name: '搜索进程、PID 或地址' }).fill('chrome');
  await expect(page.getByText('Code.exe', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '详情与代理' }).click();
  await expect(page.getByRole('dialog').getByRole('textbox', { name: '命令行' })).toHaveValue('chrome.exe --profile work');
  await page.getByRole('button', { name: '应用到进程树' }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('手动 / 继承')).toBeVisible();
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-processes.png') });
  await page.getByRole('link', { name: '路由规则', exact: true }).click();
  await page.getByRole('button', { name: '新建规则' }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('textbox', { name: '规则名称' }).fill('浏览器规则');
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(dialog.getByText('操作未完成')).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: '规则名称' })).toHaveValue('浏览器规则');
  await dialog.getByRole('textbox', { name: '进程名称', exact: true }).fill('chrome.exe');
  await dialog.getByRole('textbox', { name: '包含目标端口' }).fill('443\n8443');
  await dialog.getByRole('checkbox', { name: '启用这条规则' }).check();
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText('浏览器规则', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '编辑', exact: true }).click();
  await expect(page.getByRole('textbox', { name: '包含目标端口' })).toHaveValue('443\n8443');
  await page.getByRole('button', { name: '取消', exact: true }).click();
  await page.getByRole('link', { name: '代理组', exact: true }).click();
  await page.getByRole('button', { name: '测试连接' }).click();
  await expect(page.getByText('SOCKS5 连接被拒绝')).toBeVisible();
  await page.getByRole('link', { name: '设置', exact: true }).click();
  await page.getByRole('combobox', { name: '主题' }).click();
  await page.getByRole('option', { name: '深色', exact: true }).click();
  await page.getByRole('button', { name: '保存设置' }).click();
  await expect(page.getByRole('combobox', { name: '主题' })).toContainText('深色');
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-settings-dark.png') });
  await page.setViewportSize({ width: 760, height: 560 });
  await page.screenshot({ path: join(tmpdir(), 'rs-proxy-settings-narrow.png') });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBeTruthy();
  expect(externalFonts).toEqual([]); expect(errors).toEqual([]);
});
