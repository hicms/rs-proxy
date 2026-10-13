import { expect, type Page } from '@playwright/test';
import type { Snapshot } from '../../src/bindings/Snapshot';
import { readFileSync } from 'node:fs';

const fixture: Snapshot = {
  traffic: { total: { uploaded_bytes: 0, downloaded_bytes: 0, upload_bytes_per_second: 0, download_bytes_per_second: 0 }, groups: [], processes: [] },
  config: { proxy_process_dns: true, version: 1, revision: 0, proxy_groups: [{ id: 0, name: '本地代理', host: '127.0.0.1', port: 7890 }], auto_rules: [], default_exclude_cidrs: ['127.0.0.0/8', '10.0.0.0/8'], dns: { enabled: false, upstream_host: '8.8.8.8', upstream_port: 53, listen_host: '127.0.0.2', listen_port: 53, proxy_group_id: 0 }, ui: { theme: 'light', close_to_tray: true }, log_level: 'info', tcp_syn_parking: { watchdog_ms: 20, pool_size: 256 } },
  processes: ['Code.exe', 'chrome.exe', 'pwsh.exe'].map((name, index) => ({ process: { identity: { pid: 100 + index, sequence: String(index + 1) }, parent: null, name, image_path: `C:\\Apps\\${name}`, cmdline: `${name} --profile work` }, route: null })),
  connections: [{ pid: 101, protocol: 'tcp', local_address: '10.0.0.2', local_port: 50123, remote_address: '192.0.2.1', remote_port: 443, state: 'ESTABLISHED' }],
  stats: { running: true, elevated: true, dns_active: false, tcp_connections: 1, udp_sessions: 0, proxied_packets: 100, direct_packets: 10, uploaded_bytes: 1024, downloaded_bytes: 8192, parked_syns: 1, timed_out_syns: 0, last_error: null },
};
export async function desktop(page: Page, options: { iconDelay?: number; processCount?: number; brokenIcon?: boolean } = {}) {
  const icons = [`data:image/png;base64,${readFileSync('src-tauri/icons/icon.png').toString('base64')}`, 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLttAAAAABJRU5ErkJggg=='];
  await page.addInitScript(({ snapshot, options, icons }) => {
    if (options.processCount) snapshot.processes = Array.from({ length: options.processCount }, (_, index) => ({ ...snapshot.processes[index % 3], process: { ...snapshot.processes[index % 3].process, identity: { pid: 100 + index, sequence: String(index + 1) } } }));
    const scope = window as unknown as Record<string, unknown>;
    scope.iconCalls = []; scope.iconActive = 0; scope.iconMaxActive = 0; scope.snapshot = snapshot; scope.reconnectCalls = [];
    scope.isTauri = true;
    scope.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    scope.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command: string, args: Record<string, unknown> = {}) => {
        if (command.startsWith('plugin:event|')) return 1;
        if (command === 'get_snapshot') return structuredClone(snapshot);
        if (command === 'get_process_icons') {
          const identities = args.identities as { pid: number; sequence: string }[];
          (scope.iconCalls as unknown[]).push(identities);
          scope.iconActive = Number(scope.iconActive) + 1;
          scope.iconMaxActive = Math.max(Number(scope.iconMaxActive), Number(scope.iconActive));
          try {
            await new Promise(resolve => setTimeout(resolve, options.iconDelay ?? 0));
            if (scope.iconError) throw '图标服务不可用';
            return identities.map(identity => ({ identity, data_url: (identity.pid - 100) % 3 === 2 ? null : options.brokenIcon ? 'data:image/png;base64,broken' : icons[(identity.pid - 100) % 3] }));
          } finally { scope.iconActive = Number(scope.iconActive) - 1; }
        }
        if (command === 'save_config') {
          if (scope.saveDelay) await new Promise(resolve => setTimeout(resolve, Number(scope.saveDelay)));
          const config = args.config as typeof snapshot.config;
          if (config.auto_rules.some(rule => !rule.name || (!rule.process_name && !rule.image_path_pattern && !rule.cmdline_pattern))) throw '规则至少需要一个进程匹配条件';
          if (config.revision !== snapshot.config.revision) throw '配置已被更新';
          snapshot.config = { ...config, revision: config.revision + 1 }; return snapshot.config;
        }
        if (command === 'get_autostart') return scope.autostart ?? { enabled: false, start_minimized: false };
        if (command === 'set_autostart') {
          if (scope.saveDelay) await new Promise(resolve => setTimeout(resolve, Number(scope.saveDelay)));
          return scope.autostart = { enabled: args.enabled, start_minimized: args.startMinimized };
        }
        if (command === 'set_engine_running') { snapshot.stats.running = Boolean(args.running); return; }
        if (command === 'set_process_proxy') {
          const id = args.identity as { pid: number; sequence: string };
          const process = snapshot.processes.find(item => item.process.identity.pid === id.pid && item.process.identity.sequence === id.sequence);
          if (!process) throw '进程身份已变化';
          process.route = args.groupId === null ? null : { group_id: Number(args.groupId), rule_id: null, source: 'manual' }; return;
        }
        if (command === 'test_proxy') throw 'SOCKS5 连接被拒绝';
        if (command === 'reconnect_processes') {
          (scope.reconnectCalls as unknown[]).push(structuredClone(args));
          if (scope.reconnectDelay) await new Promise(resolve => setTimeout(resolve, Number(scope.reconnectDelay)));
          if (scope.reconnectError) throw scope.reconnectError;
          return scope.reconnectResult ?? { disconnected: 2, skipped: 1, failures: [] };
        }
        if (command === 'delete_group') {
          if (args.revision !== snapshot.config.revision) throw '配置已被更新';
          snapshot.config.proxy_groups = snapshot.config.proxy_groups.filter(group => group.id !== args.from);
          snapshot.config.dns.proxy_group_id = Number(args.to); snapshot.config.revision += 1;
          return snapshot.config;
        }
        throw new Error(`Unexpected command ${command}`);
      },
    };
  }, { snapshot: fixture, options, icons });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: '进程与连接', exact: true })).toBeVisible();
}
