// Copyright (c) Meta Platforms, Inc. and affiliates.
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Layout, LayoutHeader, LayoutContent, HStack, VStack } from '@astryxdesign/core/Layout';
import { Text, Heading } from '@astryxdesign/core/Text';
import { Table, proportional, type TableColumn } from '@astryxdesign/core/Table';
import { Button } from '@astryxdesign/core/Button';
import { TextInput } from '@astryxdesign/core/TextInput';
import { NumberInput } from '@astryxdesign/core/NumberInput';
import { Banner } from '@astryxdesign/core/Banner';
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
  const columns: TableColumn<ProxyGroup>[] = [
    { key: 'name', header: '名称', width: proportional(2), renderCell: group => <Text weight="semibold">{group.name}</Text> },
    { key: 'host', header: 'SOCKS5 服务器', width: proportional(2), renderCell: group => <Text type="code">{group.host}:{group.port}</Text> },
    { key: 'references', header: '规则引用', renderCell: group => <Text>{config.auto_rules.filter(rule => rule.proxy_group_id === group.id).length}</Text> },
    { key: 'test', header: '最近测试', width: proportional(2), renderCell: group => <Text type="supporting">{results[group.id] ?? '尚未测试'}</Text> },
    { key: 'actions', header: '操作', width: proportional(3), renderCell: group => <HStack gap={2} wrap="wrap"><Button label="测试连接" size="sm" isDisabled={action.busy} onClick={() => void action.run(async () => { try { const result = await invoke<ProxyTestResult>('test_proxy', { group }); setResults(previous => ({ ...previous, [group.id]: `${result.latency_ms} ms · 握手成功` })); } catch (error) { setResults(previous => ({ ...previous, [group.id]: '测试失败' })); throw error; } })} /><Button label="编辑" size="sm" onClick={() => setEditing({ config, group })} /><Button label="删除" size="sm" variant="ghost" isDisabled={config.proxy_groups.length === 1} onClick={() => { setDeleting(group); setReplacement(config.proxy_groups.find(item => item.id !== group.id)!.id); }} /></HStack> },
  ];
  return <><Layout header={<LayoutHeader hasDivider><HStack vAlign="center" hAlign="between" gap={3} wrap="wrap"><VStack gap={1}><Heading level={1}>代理组</Heading><Text color="secondary">为不同进程选择 SOCKS5 服务器。</Text></VStack><Button label="新建代理组" variant="primary" onClick={() => setEditing({ config, group: { id: Math.max(...config.proxy_groups.map(group => group.id)) + 1, name: '', host: '127.0.0.1', port: 7890 } })} /></HStack></LayoutHeader>}
    content={<LayoutContent><VStack gap={4}>{action.error && <Banner status="error" title="代理操作失败" description={action.error} />}<Table data={config.proxy_groups} columns={columns} idKey="id" hasHover /></VStack></LayoutContent>} />
    {editing && <ProxyEditor config={editing.config} initial={editing.group} onClose={() => setEditing(undefined)} />}
    {deleting && <Editor title="删除代理组" saveLabel="替换引用并删除" destructive onClose={() => setDeleting(undefined)} onSave={async () => {
      await invoke('delete_group', { from: deleting.id, to: replacement, revision: config.revision });
      await model.refresh();
    }}><Text>将「{deleting.name}」的手动选择、规则和 DNS 引用改用以下代理组，然后删除此组。已有 TCP 连接会继续使用原服务器。</Text><GroupSelector label="替换为" groups={config.proxy_groups.filter(group => group.id !== deleting.id)} value={replacement} onChange={setReplacement} /></Editor>}
  </>;
}
