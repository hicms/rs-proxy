// Copyright (c) Meta Platforms, Inc. and affiliates.
import { memo, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Layout, LayoutHeader, LayoutContent, LayoutFooter, HStack, VStack, StackItem } from '@astryxdesign/core/Layout';
import { Text, Heading } from '@astryxdesign/core/Text';
import { Button } from '@astryxdesign/core/Button';
import { Table, proportional, type TableColumn } from '@astryxdesign/core/Table';
import { TextInput } from '@astryxdesign/core/TextInput';
import { TextArea } from '@astryxdesign/core/TextArea';
import { SegmentedControl, SegmentedControlItem } from '@astryxdesign/core/SegmentedControl';
import { Token } from '@astryxdesign/core/Token';
import { TabList, Tab } from '@astryxdesign/core/TabList';
import { Pagination } from '@astryxdesign/core/Pagination';
import { Banner } from '@astryxdesign/core/Banner';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { Icon } from '@astryxdesign/core/Icon';
import { IconButton } from '@astryxdesign/core/IconButton';
import { Selector } from '@astryxdesign/core/Selector';
import { Tooltip } from '@astryxdesign/core/Tooltip';
import { AppWindow, ArrowRight, RefreshCw, ListFilter, Network, Search, Undo2, Unplug } from 'lucide-react';
import { useAnnounce } from '@astryxdesign/core/hooks';
import { GroupSelector, Editor } from '../components/Editor';
import { Connections } from '../components/Connections';
import { ProcessIcon } from '../components/ProcessIcon';
import { TrafficRate, TrafficSummary } from '../components/Traffic';
import { processKey, useProcessIcons } from '../processIcons';
import { useAppModel, useAction } from '../model';
import type { Snapshot } from '../bindings/Snapshot';
import type { ProcessView } from '../bindings/ProcessView';
import type { Route } from '../bindings/Route';
import type { ProcessIdentity } from '../bindings/ProcessIdentity';
import type { ReconnectResult } from '../bindings/ReconnectResult';

const ProcessTable = memo(Table<ProcessView>);
const processRowKey = (item: ProcessView) => processKey(item.process.identity);

function proxySource(route: Route | null, ruleNames: Map<string, string>) {
  if (!route) return '直连';
  if (route.source === 'manual') return '手动选择（含继承）';
  return (route.rule_id && ruleNames.get(route.rule_id)) || '规则已不可用';
}

export default function Processes({ snapshot }: { snapshot: Snapshot }) {
  const [query, setQuery] = useState(''); const [scope, setScope] = useState('all'); const [tab, setTab] = useState('processes'); const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<ProcessView>(); const [group, setGroup] = useState(snapshot.config.proxy_groups[0].id);
  const [groupFilter, setGroupFilter] = useState('all');
  const [reconnect, setReconnect] = useState<{ identities: ProcessIdentity[]; groupId: number | null; scope: string }>();
  const [reconnectResult, setReconnectResult] = useState<ReconnectResult>();
  const announce = useAnnounce();
  const filterGroup = snapshot.config.proxy_groups.find(item => String(item.id) === groupFilter);
  if (groupFilter !== 'all' && !filterGroup) { setGroupFilter('all'); setPage(1); }
  const activeGroup = tab === 'processes' ? filterGroup : undefined;
  const traffic = useMemo(() => new Map(snapshot.traffic.processes.map(item => [processKey(item.identity), item])), [snapshot.traffic.processes]);
  const summary = activeGroup ? snapshot.traffic.groups.find(item => item.group_id === activeGroup.id)?.stats : snapshot.traffic.total;
  const model = useAppModel(); const action = useAction();
  const ruleNames = useMemo(() => new Map(snapshot.config.auto_rules.map(rule => [rule.id, rule.name])), [snapshot.config.auto_rules]);
  const currentSelection = selected && snapshot.processes.find(item => processKey(item.process.identity) === processKey(selected.process.identity));
  const rows = useMemo(() => snapshot.processes.filter(item => (scope !== 'proxied' || item.route) && (!filterGroup || item.route?.group_id === filterGroup.id) && `${item.process.name} ${item.process.identity.pid} ${item.process.image_path} ${item.process.cmdline}`.toLowerCase().includes(query.toLowerCase())), [snapshot.processes, query, scope, filterGroup]);
  const reconnectTargets = rows.filter(item => item.route);
  const connections = useMemo(() => snapshot.connections.filter(item => `${item.pid} ${item.local_address} ${item.remote_address} ${item.protocol}`.toLowerCase().includes(query.toLowerCase())), [snapshot.connections, query]);
  const total = tab === 'processes' ? rows.length : connections.length; const current = Math.min(page, Math.max(1, Math.ceil(total / 50)));
  const processStart = (Math.min(page, Math.max(1, Math.ceil(rows.length / 50))) - 1) * 50;
  const connectionStart = (Math.min(page, Math.max(1, Math.ceil(connections.length / 50))) - 1) * 50;
  const visibleProcesses = useMemo(() => rows.slice(processStart, processStart + 50), [rows, processStart]);
  const visibleConnections = useMemo(() => connections.slice(connectionStart, connectionStart + 50), [connections, connectionStart]);
  const icons = useProcessIcons(visibleProcesses);
  const columns = useMemo<TableColumn<ProcessView>[]>(() => [
    { key: 'name', header: '应用', width: proportional(3), renderCell: item => <HStack gap={3} vAlign="center"><ProcessIcon dataUrl={icons?.get(processKey(item.process.identity))} /><VStack gap={0.5}><Text weight="semibold" maxLines={1}>{item.process.name}</Text><Text type="supporting" color="secondary">PID {item.process.identity.pid}</Text></VStack></HStack> },
    { key: 'route', header: '连接方式', width: proportional(2), renderCell: item => item.route ? <Token label={snapshot.config.proxy_groups.find(group => group.id === item.route?.group_id)?.name ?? '代理'} color="green" size="sm" /> : <Text color="secondary">直连</Text> },
    { key: 'source', header: '代理来源', width: proportional(2), renderCell: item => <Tooltip content={proxySource(item.route, ruleNames)} isEnabled={Boolean(item.route)} hasHoverIndication={false}><Text maxLines={2} hasTruncateTooltip={false} color={item.route ? 'primary' : 'secondary'}>{proxySource(item.route, ruleNames)}</Text></Tooltip> },
    { key: 'traffic', header: '实时流量', width: proportional(2), renderCell: item => { const stats = traffic.get(processKey(item.process.identity)); return <TrafficRate stats={filterGroup ? stats?.groups.find(group => group.group_id === filterGroup.id)?.stats : stats?.total} />; } },
    { key: 'actions', header: '', align: 'end', renderCell: item => <IconButton label="详情与代理" tooltip="查看详情与代理设置" icon={<Icon icon={ArrowRight} />} variant="ghost" size="sm" onClick={() => { setSelected(item); setGroup(item.route?.group_id ?? snapshot.config.proxy_groups[0].id); }} /> },
  ], [snapshot.config.proxy_groups, ruleNames, icons, filterGroup, traffic]);
  return <><Layout header={<LayoutHeader hasDivider padding={3}><VStack gap={2}>
    <HStack vAlign="center" gap={4}><Heading level={1} textWrap="nowrap">进程与连接</Heading>
    <StackItem size="fill"><TrafficSummary compact stats={summary} label={`${activeGroup?.name ?? '全部代理组'} · 本次启动流量（仅代理）`} /></StackItem>
    <HStack gap={1} vAlign="center">
      {tab === 'processes' && <Button label="重连已选代理" icon={<Icon icon={Unplug} />} size="sm" variant="ghost" isDisabled={!snapshot.stats.running || reconnectTargets.length === 0} tooltip={!snapshot.stats.running ? '请先启动代理引擎' : '断开当前筛选范围内需要切换到代理的旧连接'} onClick={() => setReconnect({ identities: reconnectTargets.map(item => item.process.identity), groupId: filterGroup?.id ?? null, scope: `${filterGroup?.name ?? '全部代理组'}${query ? ` · 搜索“${query}”` : ''}` })} />}
      <IconButton label="立即刷新" tooltip="立即刷新" icon={<Icon icon={RefreshCw} />} variant="ghost" size="sm" onClick={() => void model.refresh()} />
    </HStack></HStack>
    {reconnectResult && <Banner status={reconnectResult.failures.length ? 'warning' : reconnectResult.disconnected ? 'success' : 'info'} title={reconnectResult.disconnected ? `已断开 ${reconnectResult.disconnected} 条旧连接` : reconnectResult.failures.length ? '有连接未能处理' : '没有需要断开的旧连接'} description={`保留或已结束 ${reconnectResult.skipped} 条连接。${reconnectResult.failures.length ? `${reconnectResult.failures.length} 项未能处理。` : ''}${reconnectResult.disconnected ? '应用新建连接后将按当前规则使用代理；未自动恢复时，请刷新页面或重试。' : '已代理的连接会继续保留。'}`} isDismissable onDismiss={() => setReconnectResult(undefined)}>
      {reconnectResult.failures.length > 0 && <TextArea label="未处理详情" value={reconnectResult.failures.join('\n')} isReadOnly rows={3} />}
    </Banner>}
    <HStack vAlign="center" gap={3} wrap="wrap"><TabList size="sm" value={tab} onChange={value => { setTab(value); setPage(1); }}><Tab value="processes" label="进程" icon={<Icon icon={AppWindow} />} /><Tab value="connections" label="连接" icon={<Icon icon={Network} />} /></TabList>
      <StackItem size="fill"><TextInput label="搜索进程、PID 或地址" isLabelHidden placeholder="搜索应用、PID 或地址…" value={query} onChange={value => { setQuery(value); setPage(1); }} startIcon="search" hasClear /></StackItem>
      {tab === 'processes' && <SegmentedControl label="显示范围" value={scope} onChange={value => { setScope(value); setPage(1); }}><SegmentedControlItem value="all" label="全部" /><SegmentedControlItem value="proxied" label="已选代理" icon={<Icon icon={ListFilter} />} /></SegmentedControl>}
      {tab === 'processes' && <Selector label="筛选代理组" isLabelHidden value={filterGroup ? groupFilter : 'all'} onChange={value => { setGroupFilter(value); setPage(1); }} options={[{ value: 'all', label: '全部代理组' }, ...snapshot.config.proxy_groups.map(group => ({ value: String(group.id), label: group.name }))]} variant="ghost" hasSearch startIcon={<Icon icon={Network} />} />}
    </HStack>
  </VStack></LayoutHeader>} content={<LayoutContent>
    <VStack style={{ display: tab === 'processes' ? undefined : 'none' }}>{rows.length ? <ProcessTable data={visibleProcesses} columns={columns} idKey={processRowKey} rowCount={rows.length} rowIndexStart={processStart + 1} density="balanced" hasHover /> : <EmptyState icon={<Icon icon={Search} size="lg" />} title="没有匹配的进程" description="调整搜索条件或显示范围。" />}</VStack>
    <VStack style={{ display: tab === 'connections' ? undefined : 'none' }}><Connections data={visibleConnections} /></VStack>
  </LayoutContent>} footer={<LayoutFooter hasDivider><HStack hAlign="between" vAlign="center" wrap="wrap" gap={2}><HStack gap={3} vAlign="center"><Text type="supporting">共 {total} 项 · 每秒更新</Text><Text type="supporting" color="secondary">{snapshot.stats.tcp_connections} TCP · {snapshot.stats.udp_sessions} UDP</Text></HStack><Pagination label="列表分页" page={current} onChange={setPage} totalItems={total} pageSize={50} variant="compact" /></HStack></LayoutFooter>} />
    {selected && <Editor title={selected.process.name} saveLabel="应用到进程树" onClose={() => setSelected(undefined)} onSave={async () => { await invoke('set_process_proxy', { identity: selected.process.identity, groupId: group }); await model.refresh(); }}>
      {action.error && <Banner status="error" title="操作未完成" description={action.error} />}
      <Text color="secondary">PID {selected.process.identity.pid} · 父进程 {selected.process.parent?.pid ?? '—'}。选择会覆盖此进程树的自动规则。</Text>
      <Text>当前代理来源：{currentSelection ? proxySource(currentSelection.route, ruleNames) : '进程已结束'}</Text>
      <TrafficSummary stats={traffic.get(processKey(selected.process.identity))?.total} label="此进程 · 本次启动流量（仅代理）" />
      <TextInput label="程序路径" value={selected.process.image_path} isReadOnly />
      <TextArea label="命令行" value={selected.process.cmdline || '无法读取'} isReadOnly rows={3} />
      <GroupSelector groups={snapshot.config.proxy_groups} value={group} onChange={setGroup} />
      <Button label="移除手动选择，使用自动规则" icon={<Icon icon={Undo2} />} variant="ghost" isLoading={action.busy} onClick={() => void action.run(async () => { await invoke('set_process_proxy', { identity: selected.process.identity, groupId: null }); await model.refresh(); setSelected(undefined); })} />
      <Heading level={3}>当前连接</Heading><Connections data={snapshot.connections.filter(item => item.pid === selected.process.identity.pid)} />
    </Editor>}
    {reconnect && <Editor title="重连已选代理" saveLabel="断开旧连接" onClose={() => setReconnect(undefined)} onSave={async () => {
      const result = await invoke<ReconnectResult>('reconnect_processes', { identities: reconnect.identities, groupId: reconnect.groupId });
      setReconnectResult(result);
      announce(`已断开 ${result.disconnected} 条旧连接，${result.failures.length} 项未能处理`);
      await model.refresh();
    }}>
      <Text weight="semibold">{reconnect.scope} · {reconnect.identities.length} 个已选代理进程（全部分页）</Text>
      <Text>仅断开这些进程中符合当前代理规则的旧直连连接。进程会继续运行，已代理的连接会保留。</Text>
      <Banner status="warning" title="连接会短暂中断" description="下载、通话等任务可能需要重试。应用重新建立连接后才会走代理；如果未自动恢复，请刷新页面或重试。" />
      <Text type="supporting" color="secondary">仅处理 IPv4 TCP 连接，不处理 UDP 和 IPv6。此操作使用已生效的代理设置。</Text>
    </Editor>}
  </>;
}
