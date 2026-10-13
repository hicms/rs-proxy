import { chromium, expect } from '@playwright/test';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { writeFile, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { observeSocks } from './socks-observer.mjs';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { focusExistingInstance } from './second-instance.mjs';

const evidence = join(tmpdir(), 'rs-proxy-e2e');
const applicationPid = Number(process.env.RS_PROXY_E2E_PID);
const windows = (action, value = '', pid = applicationPid) => execFileSync(
  `${process.env.SystemRoot}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe`,
  ['-NoProfile', '-NonInteractive', '-File', resolve('tests/native/windows.ps1'), '-Action', action, '-ProcessId', String(pid), '-Value', value],
  { encoding: 'utf8', windowsHide: true, timeout: 15000 },
);
const records = []; const errors = []; const failures = [];
let browser, page, client, clientLines, observerA, observerB;
const rpc = (command, args = {}) => page.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const snapshot = () => rpc('get_snapshot');
const nav = name => page.getByRole('link', { name, exact: true }).click();
const button = name => page.getByRole('button', { name, exact: true });
const dialog = () => page.getByRole('dialog');
async function check(name, action, keepGoing = false) {
  try { await action(); records.push({ name, passed: true }); console.log(`PASS ${name}`); }
  catch (error) { records.push({ name, passed: false, error: String(error) }); if (!keepGoing) throw error; failures.push(error); console.error(`FAIL ${name}: ${error}`); }
  finally { await writeFile(join(evidence, 'checks.json'), JSON.stringify(records, null, 2)); }
}
async function choose(label, value, scope = page) {
  await scope.getByRole('combobox', { name: label, exact: true }).click();
  await page.getByRole('option', { name: value, exact: true }).click();
}
async function saveDialog() { await dialog().getByRole('button', { name: '保存', exact: true }).click(); await expect(dialog()).toHaveCount(0); }
async function group(name, port) {
  await nav('代理组'); await button('新建代理组').click();
  await dialog().getByRole('textbox', { name: '名称 必填', exact: true }).fill(name);
  await dialog().getByRole('spinbutton', { name: '端口', exact: true }).fill(String(port));
  await saveDialog();
}
async function processDialog(pid) {
  await nav('进程与连接'); await page.getByRole('textbox', { name: '搜索进程、PID 或地址' }).fill(String(pid));
  const row = page.getByRole('row').filter({ has: page.getByText(`PID ${pid}`, { exact: true }) });
  await row.getByRole('button', { name: '详情与代理' }).click();
}
async function manual(pid, groupName) {
  await processDialog(pid); await choose('代理组', groupName, dialog());
  await button('应用到进程树').click(); await expect(dialog()).toHaveCount(0);
}
async function request(command) {
  client.stdin.write(`${JSON.stringify(command)}\n`);
  const next = await Promise.race([clientLines.next(), new Promise((_, reject) => setTimeout(() => reject(new Error('independent client timed out')), 18000).unref())]);
  const response = JSON.parse(next.value); assert.ok(response.ok, response.error); return response.result;
}
async function httpsThrough(observer, kind = 'https') {
  const start = observer.records.length;
  const command = { kind: 'https', nonce: `${Date.now()}-${start}` };
  const result = await request(kind === 'child' ? { kind: 'child', request: command } : command);
  if (kind === 'child') assert.ok(result.ok, result.error);
  await expect.poll(() => observer.records.slice(start).some(record => record.command === 1 && record.target === '1.1.1.1:443' && record.reply === 0 && record.uploaded > 0 && record.downloaded > 0)).toBeTruthy();
}
async function direct() {
  const before = [observerA.records.length, observerB.records.length];
  assert.equal((await request({ kind: 'https', nonce: `direct-${Date.now()}` })).status, 200);
  assert.deepEqual([observerA.records.length, observerB.records.length], before);
}
async function rule(name, groupName) {
  await nav('路由规则'); await button('新建规则').click();
  await dialog().getByRole('textbox', { name: '规则名称' }).fill(name);
  await dialog().getByRole('textbox', { name: '进程名称', exact: true }).fill('node.exe');
  await dialog().getByRole('textbox', { name: '命令行条件' }).fill('rs-proxy-e2e-client');
  await choose('代理组', groupName, dialog());
  await dialog().getByRole('checkbox', { name: '启用这条规则' }).check();
  await saveDialog();
}
const ruleRow = name => page.getByRole('row').filter({ has: page.getByText(name, { exact: true }) });

try {
  for (let attempt = 0; attempt < 100; attempt++) {
    try { browser = await chromium.connectOverCDP('http://127.0.0.1:9225'); break; }
    catch { await new Promise(resolve => setTimeout(resolve, 200)); }
  }
  assert.ok(browser, 'WebView2 CDP did not start');
  page = browser.contexts()[0].pages()[0] ?? await browser.contexts()[0].waitForEvent('page');
  assert.ok(page, 'native WebView page missing'); page.setDefaultTimeout(12000);
  page.on('pageerror', error => errors.push(error.message));
  await expect(page.getByRole('heading', { name: '进程与连接', exact: true })).toBeVisible();
  await expect.poll(async () => (await snapshot()).stats.running).toBeTruthy();
  assert.equal((await snapshot()).config.dns.enabled, false);
  observerA = await observeSocks(); observerB = await observeSocks();
  client = spawn(process.execPath, [resolve('tests/native/network-client.mjs'), '--rs-proxy-e2e-client'], { stdio: ['pipe', 'pipe', 'inherit'], windowsHide: true });
  clientLines = createInterface({ input: client.stdout })[Symbol.asyncIterator]();
  const ready = JSON.parse((await clientLines.next()).value); assert.ok(ready.ready);

  await check('native identity, real process list, initial screenshot', async () => {
    assert.equal(await page.title(), 'RS Proxy'); assert.ok((await snapshot()).processes.length > 10);
    await expect.poll(() => page.getByRole('row').locator('svg image').count()).toBeGreaterThan(1);
    await page.screenshot({ path: join(evidence, 'processes.png') });
    await button('转到下一页').click();
    await expect.poll(async () => Number(await page.getByRole('row').nth(1).getAttribute('aria-rowindex'))).toBeGreaterThan(50);
    await button('转到上一页').click();
  });
  await check('real process icons, PNG decoding, stale identity and batch limit', async () => {
    // The engine reconciles the process list every 30 seconds if an ETW event is delayed.
    await expect.poll(async () => (await snapshot()).processes.some(item => item.process.identity.pid === ready.pid), { timeout: 35000 }).toBeTruthy();
    const processes = (await snapshot()).processes;
    const own = processes.find(item => item.process.identity.pid === applicationPid).process.identity;
    const node = processes.find(item => item.process.identity.pid === ready.pid).process.identity;
    const icons = await rpc('get_process_icons', { identities: [own, node, { ...own, sequence: 'invalid' }] });
    assert.ok(icons[0].data_url.startsWith('data:image/png;base64,'));
    assert.ok(icons[1].data_url.startsWith('data:image/png;base64,'));
    assert.notEqual(icons[0].data_url, icons[1].data_url);
    assert.equal(icons[2].data_url, null);
    await assert.rejects(rpc('get_process_icons', { identities: Array(51).fill(own) }), /50/);
    const decoded = await page.evaluate(urls => Promise.all(urls.map(url => new Promise((resolve, reject) => {
      const image = new Image(); image.onload = () => resolve([image.naturalWidth, image.naturalHeight]); image.onerror = () => reject(new Error('Invalid native PNG')); image.src = url;
    }))), icons.slice(0, 2).map(icon => icon.data_url));
    assert.deepEqual(decoded, [[32, 32], [32, 32]]);
    const search = page.getByRole('textbox', { name: '搜索进程、PID 或地址' });
    await search.fill('');
    await expect.poll(async () => new Set(await page.getByRole('row').locator('svg image').evaluateAll(images => images.map(image => image.getAttribute('href')))).size).toBeGreaterThan(1);
    await page.screenshot({ path: join(evidence, 'process-icons.png') });
  });
  await check('native small-window rule dialog scroll and reachable actions', async () => {
    await nav('路由规则'); await button('新建规则').click();
    try {
      await page.setViewportSize({ width: 760, height: 560 });
      const save = dialog().getByRole('button', { name: '保存', exact: true });
      await expect(save).toBeInViewport({ ratio: 1 });
      await save.click();
      await expect(dialog().getByText('操作未完成', { exact: true })).toBeVisible();
      const body = dialog().locator('.astryx-layout-content');
      const footer = dialog().locator('.astryx-layout-footer');
      const before = await footer.boundingBox();
      await body.hover(); await page.mouse.wheel(0, 2000);
      await expect.poll(() => body.evaluate(element => element.scrollTop)).toBeGreaterThan(0);
      await expect(dialog().getByRole('checkbox', { name: '启用这条规则' })).toBeInViewport({ ratio: 1 });
      await expect(dialog().getByRole('heading', { name: '新建路由规则' })).toBeInViewport({ ratio: 1 });
      assert.ok(Math.abs((await footer.boundingBox()).y - before.y) < 1);
      await save.click({ trial: true });
      await page.screenshot({ path: join(evidence, 'rule-dialog-small.png') });
      await dialog().getByRole('button', { name: '取消', exact: true }).click();
      await expect(dialog()).toHaveCount(0);
    } finally { await page.setViewportSize({ width: 1280, height: 820 }); }
    await nav('进程与连接');
  });
  await check('proxy creation, real handshake, edit and validation failure', async () => {
    await group('验证 A', observerA.port); await group('验证 B', observerB.port);
    const row = page.getByRole('row').filter({ hasText: '验证 A' });
    await row.getByRole('button', { name: '测试连接' }).click(); await expect(row).toContainText('已连接');
    await row.getByRole('button', { name: '编辑', exact: true }).click();
    await dialog().getByRole('textbox', { name: '名称 必填', exact: true }).fill('');
    await button('保存').click(); await expect(dialog().getByText('操作未完成')).toBeVisible();
    await dialog().getByRole('textbox', { name: '名称 必填', exact: true }).fill('验证 A'); await saveDialog();
    await page.screenshot({ path: join(evidence, 'proxies.png') });
  });
  await check('manual process selection, real HTTPS through upstream 7891', async () => { await manual(ready.pid, '验证 A'); await httpsThrough(observerA); });
  await check('process filter, details and live connection list', async () => {
    await nav('进程与连接'); await page.getByRole('textbox', { name: '搜索进程、PID 或地址' }).fill(String(ready.pid));
    await page.getByRole('radio', { name: '已选代理', exact: true }).check();
    await expect(page.getByText(`PID ${ready.pid}`, { exact: true })).toBeVisible();
    await request({ kind: 'hold' }); await button('连接').click(); await button('立即刷新').click();
    await expect(page.getByRole('row').filter({ has: page.getByText(String(ready.pid), { exact: true }) }).first()).toContainText('TCP');
    await request({ kind: 'release' }); await button('进程').click(); await page.getByRole('radio', { name: '全部', exact: true }).check();
  });
  await check('UDP ASSOCIATE with valid DNS replies from two targets', async () => {
    const start = observerA.records.length;
    const replies = await request({ kind: 'udp', id: 5100, hosts: ['1.1.1.1', '1.0.0.1'] }); assert.equal(replies.length, 2);
    assert.ok(observerA.records.slice(start).some(record => record.command === 3 && record.reply === 0));
    const datagrams = observerA.records.slice(start).flatMap(record => record.datagrams);
    for (const [index, target] of ['1.1.1.1:53', '1.0.0.1:53'].entries()) for (const direction of ['request', 'response']) assert.ok(datagrams.some(packet => packet.direction === direction && packet.target === target && packet.id === 5100 + index), `Missing real UDP ${direction}: ${target}`);
  }, true);
  await check('manual removal restores direct connection', async () => {
    await processDialog(ready.pid); await button('移除手动选择，使用自动规则').click(); await expect(dialog()).toHaveCount(0); await direct();
  });
  await check('automatic rule matches real executable and command line', async () => {
    await rule('规则 A', '验证 A'); await httpsThrough(observerA);
    await processDialog(ready.pid);
    await expect(dialog().getByText('当前代理来源：规则 A', { exact: true })).toBeVisible();
    await dialog().getByRole('button', { name: '取消', exact: true }).click();
  });
  await check('new descendant inherits parent rule', async () => {
    const child = await request({ kind: 'spawn' }); assert.ok(child.ready); await httpsThrough(observerA, 'child');
    await processDialog(child.pid);
    await expect(dialog().getByText('当前代理来源：规则 A', { exact: true })).toBeVisible();
    await dialog().getByRole('button', { name: '取消', exact: true }).click();
  });
  await check('rule priority and enabled state change actual upstream', async () => {
    await rule('规则 B', '验证 B'); await httpsThrough(observerA);
    await ruleRow('规则 B').getByRole('button', { name: '上移' }).click(); await expect(page.getByRole('row').nth(1)).toContainText('规则 B'); await httpsThrough(observerB);
    await ruleRow('规则 B').getByRole('switch').click(); await expect(ruleRow('规则 B').getByRole('switch')).not.toBeChecked(); await httpsThrough(observerA);
    await page.screenshot({ path: join(evidence, 'rules.png') });
  });
  await check('protocol, CIDR and port filters affect real traffic', async () => {
    await ruleRow('规则 A').getByRole('button', { name: '编辑', exact: true }).click();
    await choose('协议', 'TCP', dialog()); await dialog().getByRole('textbox', { name: '包含目标网段' }).fill('1.1.1.1/32');
    await dialog().getByRole('textbox', { name: '包含目标端口' }).fill('443'); await saveDialog(); await httpsThrough(observerA);
    for (const [label, mismatch, matching] of [['包含目标网段', '8.8.8.8/32', '1.1.1.1/32'], ['包含目标端口', '80', '443']]) {
      await ruleRow('规则 A').getByRole('button', { name: '编辑', exact: true }).click(); await dialog().getByRole('textbox', { name: label }).fill(mismatch); await saveDialog(); await direct();
      await ruleRow('规则 A').getByRole('button', { name: '编辑', exact: true }).click(); await dialog().getByRole('textbox', { name: label }).fill(matching); await saveDialog();
    }
    await ruleRow('规则 A').getByRole('button', { name: '编辑', exact: true }).click(); await choose('协议', 'UDP', dialog()); await saveDialog(); await direct();
    await ruleRow('规则 A').getByRole('button', { name: '编辑', exact: true }).click(); await choose('协议', 'TCP', dialog()); await saveDialog(); await httpsThrough(observerA);
    const before = observerA.records.length; await request({ kind: 'udp', id: 5200, hosts: ['1.1.1.1'] }); assert.equal(observerA.records.length, before);
    await ruleRow('规则 A').getByRole('button', { name: '编辑', exact: true }).click();
    await dialog().getByRole('textbox', { name: '排除目标端口' }).fill('443'); await saveDialog(); await direct();
  });
  await check('rule deletion and group replacement update manual routes', async () => {
    for (const name of ['规则 A', '规则 B']) { await ruleRow(name).getByRole('button', { name: '删除', exact: true }).click(); await dialog().getByRole('button', { name: '删除规则' }).click(); await expect(dialog()).toHaveCount(0); }
    await manual(ready.pid, '验证 A'); await nav('代理组');
    await page.getByRole('row').filter({ hasText: '验证 A' }).getByRole('button', { name: '删除', exact: true }).click();
    await choose('替换为', '验证 B', dialog()); await button('替换引用并删除').click(); await expect(dialog()).toHaveCount(0); await httpsThrough(observerB);
  });
  await check('engine pause, direct access, restart', async () => {
    await button('暂停代理').click(); await expect(button('启动代理')).toBeVisible(); await direct();
    await button('启动代理').click(); await expect(button('暂停代理')).toBeVisible(); await manual(ready.pid, '验证 B'); await httpsThrough(observerB);
  });
  await check('settings theme, discard, advanced save and network defaults', async () => {
    await nav('设置'); await choose('主题', '深色'); await button('放弃修改').click(); await expect(page.getByRole('combobox', { name: '主题' })).toContainText('跟随系统');
    await choose('主题', '深色'); await button('保存设置').click(); await expect(page.getByText('设置已保存', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: '高级', exact: true }).click(); await page.getByRole('spinbutton', { name: '首包等待时间', exact: true }).fill('25'); await page.getByRole('spinbutton', { name: '首包等待时间', exact: true }).press('Tab'); await button('保存设置').click();
    await expect(page.getByRole('spinbutton', { name: '首包等待时间', exact: true })).toHaveValue('25');
    await page.getByRole('button', { name: '网络', exact: true }).click(); await expect(page.getByRole('checkbox', { name: '启用 DNS 转发' })).not.toBeChecked();
    await page.screenshot({ path: join(evidence, 'network.png') });
    await page.getByRole('button', { name: '常规', exact: true }).click(); await page.screenshot({ path: join(evidence, 'settings.png') });
  });
  await check('global exclusion and invalid settings preserve saved config', async () => {
    await button('网络').click(); const field = page.getByRole('textbox', { name: '全局排除网段', exact: true }); const original = await field.inputValue();
    const revision = (await snapshot()).config.revision;
    await field.fill('invalid-cidr'); await button('保存设置').click(); await expect(page.getByText('设置未保存', { exact: true })).toBeVisible(); assert.equal((await snapshot()).config.revision, revision);
    await field.fill(`${original}\n1.1.1.1/32`); await button('保存设置').click();
    await expect.poll(async () => (await snapshot()).config.default_exclude_cidrs).toContain('1.1.1.1/32');
    await expect(field).toBeEnabled(); await direct();
    await field.fill(original); await button('保存设置').click();
    await expect.poll(async () => (await snapshot()).config.default_exclude_cidrs).toEqual(original.split('\n').filter(Boolean));
    await expect(field).toBeEnabled(); await httpsThrough(observerB); await button('常规').click();
  });
  await check('failed proxy handshake is shown in the real UI', async () => {
    const server = createServer(); server.listen(0, '127.0.0.1'); await once(server, 'listening'); const port = server.address().port; server.close(); await once(server, 'close');
    await group('离线测试', port); const row = page.getByRole('row').filter({ hasText: '离线测试' });
    await row.getByRole('button', { name: '测试连接' }).click(); await expect(row).toContainText('连接失败');
    await row.getByRole('button', { name: '删除', exact: true }).click(); await choose('替换为', '验证 B', dialog()); await button('替换引用并删除').click(); await expect(dialog()).toHaveCount(0); await nav('设置');
  });
  await check('automatic startup task create, read and remove', async () => {
    const original = await rpc('get_autostart');
    await expect(page.getByRole('checkbox', { name: '登录 Windows 时启动' })).toBeEnabled({ timeout: 30000 });
    if (original.enabled) { await page.getByRole('checkbox', { name: '登录 Windows 时启动' }).uncheck(); await button('应用启动设置').click(); await expect(page.getByRole('checkbox', { name: '登录 Windows 时启动' })).toBeEnabled({ timeout: 30000 }); }
    await page.getByRole('checkbox', { name: '登录 Windows 时启动' }).check(); await page.getByRole('checkbox', { name: '启动后收起到托盘' }).check(); await button('应用启动设置').click();
    await expect.poll(() => rpc('get_autostart')).toEqual({ enabled: true, start_minimized: true });
    await expect(page.getByRole('checkbox', { name: '登录 Windows 时启动' })).toBeEnabled({ timeout: 30000 });
    await page.getByRole('checkbox', { name: '登录 Windows 时启动' }).uncheck(); await button('应用启动设置').click(); await expect.poll(async () => (await rpc('get_autostart')).enabled).toBeFalsy();
  });
  await check('native export, import, cancel and invalid file preserve config', async () => {
    const path = join(evidence, `config-${Date.now()}.json`);
    await button('导出配置').click(); await new Promise(resolve => setTimeout(resolve, 800)); windows('file', path);
    await expect.poll(async () => { try { return JSON.parse(await readFile(path, 'utf8')).version; } catch { return 0; } }).toBe(1);
    const exported = JSON.parse(await readFile(path, 'utf8')); exported.ui.theme = 'light'; assert.equal(exported.dns.enabled, false); await writeFile(path, JSON.stringify(exported));
    await button('导入配置').click(); await new Promise(resolve => setTimeout(resolve, 500)); windows('file', path); await expect(page.getByRole('combobox', { name: '主题' })).toContainText('浅色');
    const revision = (await snapshot()).config.revision;
    await button('导入配置').click(); await new Promise(resolve => setTimeout(resolve, 500)); windows('cancel'); assert.equal((await snapshot()).config.revision, revision);
    await writeFile(path, '{invalid JSON'); await button('导入配置').click(); await new Promise(resolve => setTimeout(resolve, 500)); windows('file', path);
    await expect(page.getByText('设置未保存', { exact: true })).toBeVisible(); assert.equal((await snapshot()).config.revision, revision);
  });
  await check('close to tray and single-instance focus', async () => {
    windows('close', 'RS Proxy E2E'); await new Promise(resolve => setTimeout(resolve, 300));
    assert.ok(JSON.parse(windows('inspect')).some(window => window.Title === 'RS Proxy E2E' && !window.Visible));
    await focusExistingInstance();
    assert.ok(JSON.parse(windows('inspect')).some(window => window.Title === 'RS Proxy E2E' && window.Visible));
  });
  await check('console health and DNS remains disabled', async () => { assert.deepEqual(errors, []); assert.equal((await snapshot()).config.dns.enabled, false); assert.equal((await snapshot()).stats.dns_active, false); });
  await writeFile(join(evidence, 'upstream-records.json'), JSON.stringify({ a: observerA.records, b: observerB.records }, null, 2));
  assert.equal(failures.length, 0, 'Some independent checks failed; inspect checks.json');
} catch (error) {
  if (page) await snapshot().then(value => writeFile(join(evidence, 'failure-snapshot.json'), JSON.stringify(value, null, 2))).catch(() => {});
  await writeFile(join(evidence, 'upstream-records.json'), JSON.stringify({ a: observerA?.records, b: observerB?.records }, null, 2));
  console.error(error); if (page) await page.screenshot({ path: join(evidence, 'failure.png') }).catch(() => {}); process.exitCode = 1;
} finally {
  client?.stdin.end(); client?.kill();
  await observerA?.close(); await observerB?.close();
  if (page) await rpc('quit_app').catch(() => {});
  await browser?.close();
}
