// Copyright (c) Meta Platforms, Inc. and affiliates.
import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Layout, LayoutHeader, LayoutContent, LayoutFooter, HStack, VStack, StackItem } from '@astryxdesign/core/Layout';
import { Text, Heading } from '@astryxdesign/core/Text';
import { Button } from '@astryxdesign/core/Button';
import { Table, proportional, type TableColumn } from '@astryxdesign/core/Table';
import { TextInput } from '@astryxdesign/core/TextInput';
import { TextArea } from '@astryxdesign/core/TextArea';
import { Selector } from '@astryxdesign/core/Selector';
import { Token } from '@astryxdesign/core/Token';
import { TabList, Tab } from '@astryxdesign/core/TabList';
import { Pagination } from '@astryxdesign/core/Pagination';
import { Banner } from '@astryxdesign/core/Banner';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { GroupSelector, Editor } from '../components/Editor';
import { Connections } from '../components/Connections';
import { bytes, useAppModel, useAction } from '../model';
import type { Snapshot } from '../bindings/Snapshot';
import type { ProcessView } from '../bindings/ProcessView';

export default function Processes({ snapshot }: { snapshot: Snapshot }) {
  const [query, setQuery] = useState(''); const [scope, setScope] = useState('all'); const [tab, setTab] = useState('processes'); const [page, setPage] = useState(1);
  const [selected, setSelected] = useState<ProcessView>(); const [group, setGroup] = useState(snapshot.config.proxy_groups[0].id);
  const model = useAppModel(); const action = useAction();
  const rows = useMemo(() => snapshot.processes.filter(item => (scope !== 'proxied' || item.route) && `${item.process.name} ${item.process.identity.pid} ${item.process.image_path} ${item.process.cmdline}`.toLowerCase().includes(query.toLowerCase())), [snapshot.processes, query, scope]);
  const connections = useMemo(() => snapshot.connections.filter(item => `${item.pid} ${item.local_address} ${item.remote_address} ${item.protocol}`.toLowerCase().includes(query.toLowerCase())), [snapshot.connections, query]);
  const total = tab === 'processes' ? rows.length : connections.length; const current = Math.min(page, Math.max(1, Math.ceil(total / 50))); const start = (current - 1) * 50;
  const columns: TableColumn<ProcessView>[] = [
    { key: 'name', header: '进程', width: proportional(2), renderCell: item => <VStack gap={0.5}><Text weight="semibold" maxLines={1}>{item.process.name}</Text><Text type="supporting">PID {item.process.identity.pid} · 父进程 {item.process.parent?.pid ?? '—'}</Text></VStack> },
    { key: 'path', header: '程序路径', width: proportional(3), renderCell: item => <Text type="supporting" maxLines={1}>{item.process.image_path || '无法读取'}</Text> },
    { key: 'route', header: '策略', width: proportional(1), renderCell: item => <Token label={item.route ? snapshot.config.proxy_groups.find(group => group.id === item.route?.group_id)?.name ?? '代理' : '默认直连'} color={item.route ? 'green' : 'gray'} size="sm" /> },
    { key: 'source', header: '来源', renderCell: item => <Text type="supporting">{item.route?.source === 'manual' ? '手动 / 继承' : item.route ? '自动规则' : '—'}</Text> },
    { key: 'actions', header: '操作', renderCell: item => <Button label="详情与代理" size="sm" onClick={() => { setSelected(item); setGroup(item.route?.group_id ?? snapshot.config.proxy_groups[0].id); }} /> },
  ];
  return <><Layout header={<LayoutHeader hasDivider><VStack gap={4}>
    <HStack vAlign="center" hAlign="between" gap={3} wrap="wrap"><VStack gap={1}><Heading level={1}>进程与连接</Heading><Text color="secondary">选择进程及其后代的代理策略。规则变更作用于新连接。</Text></VStack><Button label="立即刷新" onClick={() => void model.refresh()} /></HStack>
    <HStack gap={4} wrap="wrap"><Text type="supporting">TCP 转发 {snapshot.stats.tcp_connections}</Text><Text type="supporting">UDP 会话 {snapshot.stats.udp_sessions}</Text><Text type="supporting">上传 {bytes(snapshot.stats.uploaded_bytes)}</Text><Text type="supporting">下载 {bytes(snapshot.stats.downloaded_bytes)}</Text><Text type="supporting">DNS {snapshot.stats.dns_active ? '已接管' : '系统设置'}</Text></HStack>
    <HStack vAlign="end" gap={3} wrap="wrap"><StackItem size="fill"><TextInput label="搜索进程、PID 或地址" value={query} onChange={value => { setQuery(value); setPage(1); }} startIcon="search" hasClear /></StackItem>
      {tab === 'processes' && <Selector label="显示范围" value={scope} onChange={value => { setScope(value); setPage(1); }} options={[{ value: 'all', label: '全部进程' }, { value: 'proxied', label: '已选代理' }]} />}
      <TabList value={tab} onChange={value => { setTab(value); setPage(1); }}><Tab value="processes" label="进程" /><Tab value="connections" label="连接" /></TabList>
    </HStack>
  </VStack></LayoutHeader>} content={<LayoutContent>
    {tab === 'processes' ? (rows.length ? <Table data={rows.slice(start, start + 50)} columns={columns} idKey={item => `${item.process.identity.pid}:${item.process.identity.sequence}`} rowCount={rows.length} rowIndexStart={start + 1} density="compact" hasHover /> : <EmptyState title="没有匹配的进程" description="调整搜索条件或显示范围。" />) : <Connections data={connections.slice(start, start + 50)} />}
  </LayoutContent>} footer={<LayoutFooter hasDivider><HStack hAlign="between" vAlign="center" wrap="wrap" gap={2}><Text type="supporting">共 {total} 项 · 每 2 秒更新</Text><Pagination label="列表分页" page={current} onChange={setPage} totalItems={total} pageSize={50} variant="compact" /></HStack></LayoutFooter>} />
    {selected && <Editor title={selected.process.name} saveLabel="应用到进程树" onClose={() => setSelected(undefined)} onSave={async () => { await invoke('set_process_proxy', { identity: selected.process.identity, groupId: group }); await model.refresh(); }}>
      {action.error && <Banner status="error" title="操作未完成" description={action.error} />}
      <Text color="secondary">PID {selected.process.identity.pid} · 父进程 {selected.process.parent?.pid ?? '—'}。选择会覆盖此进程树的自动规则。</Text>
      <TextInput label="程序路径" value={selected.process.image_path} isReadOnly />
      <TextArea label="命令行" value={selected.process.cmdline || '无法读取'} isReadOnly rows={3} />
      <GroupSelector groups={snapshot.config.proxy_groups} value={group} onChange={setGroup} />
      <Button label="移除手动选择，使用自动规则" isLoading={action.busy} onClick={() => void action.run(async () => { await invoke('set_process_proxy', { identity: selected.process.identity, groupId: null }); await model.refresh(); setSelected(undefined); })} />
      <Heading level={3}>当前连接</Heading><Connections data={snapshot.connections.filter(item => item.pid === selected.process.identity.pid)} />
    </Editor>}
  </>;
}
