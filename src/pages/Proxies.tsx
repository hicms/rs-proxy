// Copyright (c) Meta Platforms, Inc. and affiliates.
import { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Layout, LayoutContent, HStack, VStack } from '@astryxdesign/core/Layout';
import { Text } from '@astryxdesign/core/Text';
import { Table, proportional, type TableColumn } from '@astryxdesign/core/Table';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { StatusDot } from '@astryxdesign/core/StatusDot';
import { PageHeading, PrimaryButton } from '../components/PageHeading';
import { TextInput } from '@astryxdesign/core/TextInput';
import { NumberInput } from '@astryxdesign/core/NumberInput';
import { Banner } from '@astryxdesign/core/Banner';
import { Icon } from '@astryxdesign/core/Icon';
import { IconButton } from '@astryxdesign/core/IconButton';
import { Plus, Pencil, Trash2, Radio, Server } from 'lucide-react';
import { Editor, GroupSelector } from '../components/Editor';
import { useAction, useAppModel } from '../model';
import type { Config } from '../bindings/Config';
import type { ProxyGroup } from '../bindings/ProxyGroup';
import type { ProxyTestResult } from '../bindings/ProxyTestResult';

function ProxyEditor({ config, initial, onClose }: { config: Config; initial: ProxyGroup; onClose: () => void }) {
  const [group, setGroup] = useState(initial); const model = useAppModel();
  return <Editor title={config.proxy_groups.some(item => item.id === group.id) ? '编辑代理组' : '新建代理组'} onClose={onClose} onSave={() => model.save({ ...config, proxy_groups: config.proxy_groups.some(item => item.id === group.id) ? config.proxy_groups.map(item => item.id === group.id ? group : item) : [...config.proxy_groups, group] })}>
    <TextInput label="名称" value={group.name} onChange={name => setGroup({ ...group, name })} isRequired />
    <TextInput label="SOCKS5 主机" value={group.host} onChange={host => setGroup({ ...group, host })} description="主机名或 IP 地址，不包含协议前缀。" isRequired />
    <NumberInput label="端口" value={group.port} onChange={port => setGroup({ ...group, port })} min={1} max={65535} isIntegerOnly isWheelEnabled={false} />
    <Text type="supporting">使用 SOCKS5 无认证连接。UDP 转发要求服务器支持 UDP ASSOCIATE。</Text>
  </Editor>;
}
export default function Proxies({ config }: { config: Config }) {
  const model = useAppModel(); const action = useAction(); const [results, setResults] = useState<Record<number, string>>({});
  const [editing, setEditing] = useState<{ config: Config; group: ProxyGroup }>(); const [deleting, setDeleting] = useState<ProxyGroup>(); const [replacement, setReplacement] = useState(0);
  const [query, setQuery] = useState('');
  const rows = useMemo(() => config.proxy_groups.filter(group => `${group.name} ${group.host} ${group.port}`.toLowerCase().includes(query.toLowerCase())), [config.proxy_groups, query]);
  const columns: TableColumn<ProxyGroup>[] = [
    { key: 'name', header: '代理组', width: proportional(2), renderCell: group => <HStack gap={3} vAlign="center"><Icon icon={Server} color="secondary" /><Text weight="semibold">{group.name}</Text></HStack> },
    { key: 'host', header: 'SOCKS5 服务器', width: proportional(2), renderCell: group => <Text type="code">{group.host}:{group.port}</Text> },
    { key: 'references', header: '规则引用', renderCell: group => <Text>{config.auto_rules.filter(rule => rule.proxy_group_id === group.id).length}</Text> },
    { key: 'test', header: '最近测试', width: proportional(2), renderCell: group => <HStack gap={2} vAlign="center"><StatusDot label={results[group.id] ?? '尚未测试'} variant={!results[group.id] ? 'neutral' : results[group.id] === '连接失败' ? 'error' : 'success'} /><Text type="supporting">{results[group.id] ?? '尚未测试'}</Text></HStack> },
    { key: 'actions', header: '', width: proportional(2), renderCell: group => <HStack gap={1} hAlign="end"><IconButton label="测试连接" tooltip="测试连接" icon={<Icon icon={Radio} />} variant="ghost" size="sm" isDisabled={action.busy} onClick={() => void action.run(async () => { try { const result = await invoke<ProxyTestResult>('test_proxy', { group }); setResults(previous => ({ ...previous, [group.id]: `${result.latency_ms} ms · 已连接` })); } catch (error) { setResults(previous => ({ ...previous, [group.id]: '连接失败' })); throw error; } })} /><IconButton label="编辑" tooltip="编辑代理组" icon={<Icon icon={Pencil} />} variant="ghost" size="sm" onClick={() => setEditing({ config, group })} /><IconButton label="删除" tooltip="删除代理组" icon={<Icon icon={Trash2} />} variant="ghost" size="sm" isDisabled={config.proxy_groups.length === 1} onClick={() => { setDeleting(group); setReplacement(config.proxy_groups.find(item => item.id !== group.id)!.id); }} /></HStack> },
  ];
  return <><Layout padding={6} header={<PageHeading title="代理组" description="管理 SOCKS5 服务器与连接状态。" actions={<PrimaryButton label="新建代理组" icon={<Icon icon={Plus} />} onClick={() => setEditing({ config, group: { id: Math.max(...config.proxy_groups.map(group => group.id)) + 1, name: '', host: '127.0.0.1', port: 7890 } })} />} below={<TextInput label="搜索代理组" isLabelHidden placeholder="搜索名称、主机或端口…" value={query} onChange={setQuery} startIcon="search" hasClear />} />}
    content={<LayoutContent padding={0}><VStack gap={4} paddingInline={6}>{action.error && <Banner status="error" title="代理操作失败" description={action.error} />}{rows.length ? <Table data={rows} columns={columns} idKey="id" hasHover /> : <EmptyState icon={<Icon icon={Server} size="lg" />} title="没有匹配的代理组" description="试试其他名称、主机或端口。" />}</VStack></LayoutContent>} />
    {editing && <ProxyEditor config={editing.config} initial={editing.group} onClose={() => setEditing(undefined)} />}
    {deleting && <Editor title="删除代理组" saveLabel="替换引用并删除" destructive onClose={() => setDeleting(undefined)} onSave={() => model.changeConfig(async () => {
      await invoke('delete_group', { from: deleting.id, to: replacement, revision: config.revision });
      await model.refresh();
    })}><Text>将「{deleting.name}」的手动选择、规则和 DNS 引用改用以下代理组，然后删除此组。已有 TCP 连接会继续使用原服务器。</Text><GroupSelector label="替换为" groups={config.proxy_groups.filter(group => group.id !== deleting.id)} value={replacement} onChange={setReplacement} /></Editor>}
  </>;
}
