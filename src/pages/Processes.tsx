// Copyright (c) Meta Platforms, Inc. and affiliates.
import { useMemo, useState } from 'react';
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
import { AppWindow, ArrowRight, RefreshCw, ListFilter, Network, Search, Undo2 } from 'lucide-react';
import { GroupSelector, Editor } from '../components/Editor';
import { Connections } from '../components/Connections';
import { ProcessIcon } from '../components/ProcessIcon';
import { TrafficRate, TrafficSummary } from '../components/Traffic';
import { processKey, useProcessIcons } from '../processIcons';
import { useAppModel, useAction } from '../model';
import type { Snapshot } from '../bindings/Snapshot';
import type { ProcessView } from '../bindings/ProcessView';
import type { Route } from '../bindings/Route';

function proxySource(route: Route | null, ruleNames: Map<string, string>) {
  if (!route) return '直连';
  if (route.source === 'manual') return '手动选择（含继承）';
  return (route.rule_id && ruleNames.get(route.rule_id)) || '规则已不可用';
}

export default function Processes({ snapshot }: { snapshot: Snapshot }) {
  const [query, setQuery] = useState(''); const [scope, setScope] = useState('all'); const [tab, setTab] = useState('processes'); const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<ProcessView>(); const [group, setGroup] = useState(snapshot.config.proxy_groups[0].id);
  const [groupFilter, setGroupFilter] = useState('all');
  const filterGroup = snapshot.config.proxy_groups.find(item => String(item.id) === groupFilter);
  if (groupFilter !== 'all' && !filterGroup) { setGroupFilter('all'); setPage(1); }
  const activeGroup = tab === 'processes' ? filterGroup : undefined;
  const traffic = useMemo(() => new Map(snapshot.traffic.processes.map(item => [processKey(item.identity), item])), [snapshot.traffic.processes]);
  const summary = activeGroup ? snapshot.traffic.groups.find(item => item.group_id === activeGroup.id)?.stats : snapshot.traffic.total;
  const model = useAppModel(); const action = useAction();
  const ruleNames = useMemo(() => new Map(snapshot.config.auto_rules.map(rule => [rule.id, rule.name])), [snapshot.config.auto_rules]);
  const currentSelection = selected && snapshot.processes.find(item => processKey(item.process.identity) === processKey(selected.process.identity));
  const rows = useMemo(() => snapshot.processes.filter(item => (scope !== 'proxied' || item.route) && (!filterGroup || item.route?.group_id === filterGroup.id) && `${item.process.name} ${item.process.identity.pid} ${item.process.image_path} ${item.process.cmdline}`.toLowerCase().includes(query.toLowerCase())), [snapshot.processes, query, scope, filterGroup]);
  const connections = useMemo(() => snapshot.connections.filter(item => `${item.pid} ${item.local_address} ${item.remote_address} ${item.protocol}`.toLowerCase().includes(query.toLowerCase())), [snapshot.connections, query]);
  const total = tab === 'processes' ? rows.length : connections.length; const current = Math.min(page, Math.max(1, Math.ceil(total / 50))); const start = (current - 1) * 50;
  const visibleProcesses = tab === 'processes' ? rows.slice(start, start + 50) : [];
  const icons = useProcessIcons(visibleProcesses);
  const columns: TableColumn<ProcessView>[] = [
    { key: 'name', header: '应用', width: proportional(3), renderCell: item => <HStack gap={3} vAlign="center"><ProcessIcon dataUrl={icons?.get(processKey(item.process.identity))} /><VStack gap={0.5}><Text weight="semibold" maxLines={1}>{item.process.name}</Text><Text type="supporting" color="secondary">PID {item.process.identity.pid}</Text></VStack></HStack> },
    { key: 'route', header: '连接方式', width: proportional(2), renderCell: item => item.route ? <Token label={snapshot.config.proxy_groups.find(group => group.id === item.route?.group_id)?.name ?? '代理'} color="green" size="sm" /> : <Text color="secondary">直连</Text> },
    { key: 'source', header: '代理来源', width: proportional(2), renderCell: item => <Tooltip content={proxySource(item.route, ruleNames)} isEnabled={Boolean(item.route)} hasHoverIndication={false}><Text maxLines={2} hasTruncateTooltip={false} color={item.route ? 'primary' : 'secondary'}>{proxySource(item.route, ruleNames)}</Text></Tooltip> },
    { key: 'traffic', header: '实时流量', width: proportional(2), renderCell: item => { const stats = traffic.get(processKey(item.process.identity)); return <TrafficRate stats={filterGroup ? stats?.groups.find(group => group.group_id === filterGroup.id)?.stats : stats?.total} />; } },
    { key: 'actions', header: '', align: 'end', renderCell: item => <IconButton label="详情与代理" tooltip="查看详情与代理设置" icon={<Icon icon={ArrowRight} />} variant="ghost" size="sm" onClick={() => { setSelected(item); setGroup(item.route?.group_id ?? snapshot.config.proxy_groups[0].id); }} /> },
  ];
  return <><Layout header={<LayoutHeader hasDivider><VStack gap={4}>
    <HStack vAlign="center" hAlign="between" gap={3} wrap="wrap"><Heading level={1}>进程与连接</Heading><IconButton label="立即刷新" tooltip="立即刷新" icon={<Icon icon={RefreshCw} />} variant="ghost" onClick={() => void model.refresh()} /></HStack>
    <TrafficSummary stats={summary} label={`${activeGroup?.name ?? '全部代理组'} · 本次启动流量（仅代理）`} />
    <HStack hAlign="between" vAlign="center" gap={3} wrap="wrap"><TabList value={tab} onChange={value => { setTab(value); setPage(1); }}><Tab value="processes" label="进程" icon={<Icon icon={AppWindow} />} /><Tab value="connections" label="连接" icon={<Icon icon={Network} />} /></TabList><Text type="supporting" color="secondary">{snapshot.stats.tcp_connections} TCP · {snapshot.stats.udp_sessions} UDP</Text></HStack>
    <HStack vAlign="center" gap={3} wrap="wrap"><StackItem size="fill"><TextInput label="搜索进程、PID 或地址" isLabelHidden placeholder="搜索应用、PID 或地址…" value={query} onChange={value => { setQuery(value); setPage(1); }} startIcon="search" hasClear /></StackItem>
      {tab === 'processes' && <SegmentedControl label="显示范围" value={scope} onChange={value => { setScope(value); setPage(1); }}><SegmentedControlItem value="all" label="全部" /><SegmentedControlItem value="proxied" label="已选代理" icon={<Icon icon={ListFilter} />} /></SegmentedControl>}
      {tab === 'processes' && <Selector label="筛选代理组" isLabelHidden value={filterGroup ? groupFilter : 'all'} onChange={value => { setGroupFilter(value); setPage(1); }} options={[{ value: 'all', label: '全部代理组' }, ...snapshot.config.proxy_groups.map(group => ({ value: String(group.id), label: group.name }))]} variant="ghost" hasSearch startIcon={<Icon icon={Network} />} />}
    </HStack>
  </VStack></LayoutHeader>} content={<LayoutContent>
    {tab === 'processes' ? (rows.length ? <Table data={visibleProcesses} columns={columns} idKey={item => processKey(item.process.identity)} rowCount={rows.length} rowIndexStart={start + 1} density="balanced" hasHover /> : <EmptyState icon={<Icon icon={Search} size="lg" />} title="没有匹配的进程" description="调整搜索条件或显示范围。" />) : <Connections data={connections.slice(start, start + 50)} />}
  </LayoutContent>} footer={<LayoutFooter hasDivider><HStack hAlign="between" vAlign="center" wrap="wrap" gap={2}><Text type="supporting">共 {total} 项 · 每秒更新</Text><Pagination label="列表分页" page={current} onChange={setPage} totalItems={total} pageSize={50} variant="compact" /></HStack></LayoutFooter>} />
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
  </>;
}
