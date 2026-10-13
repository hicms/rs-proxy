// Copyright (c) Meta Platforms, Inc. and affiliates.
import { useState } from 'react';
import { Layout, LayoutHeader, LayoutContent, HStack, VStack } from '@astryxdesign/core/Layout';
import { Text, Heading } from '@astryxdesign/core/Text';
import { Table, proportional, type TableColumn } from '@astryxdesign/core/Table';
import { Button } from '@astryxdesign/core/Button';
import { TextInput } from '@astryxdesign/core/TextInput';
import { TextArea } from '@astryxdesign/core/TextArea';
import { CheckboxInput } from '@astryxdesign/core/CheckboxInput';
import { Selector } from '@astryxdesign/core/Selector';
import { Grid } from '@astryxdesign/core/Grid';
import { Token } from '@astryxdesign/core/Token';
import { Banner } from '@astryxdesign/core/Banner';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { Editor, GroupSelector } from '../components/Editor';
import { useAction, useAppModel, lines } from '../model';
import type { Config } from '../bindings/Config';
import type { AutoRule } from '../bindings/AutoRule';

function RuleEditor({ config, initial, onClose }: { config: Config; initial: AutoRule; onClose: () => void }) {
  const [rule, setRule] = useState(initial);
  const [filters, setFilters] = useState(Object.fromEntries(Object.entries(initial.dst_filter).map(([key, values]) => [key, values.join('\n')])));
  const result = () => ({ ...rule, dst_filter: { include_cidrs: lines(filters.include_cidrs), exclude_cidrs: lines(filters.exclude_cidrs), include_ports: lines(filters.include_ports), exclude_ports: lines(filters.exclude_ports) } });
  const model = useAppModel();
  const set = (patch: Partial<AutoRule>) => setRule(previous => ({ ...previous, ...patch }));
  return <Editor title={config.auto_rules.some(item => item.id === rule.id) ? '编辑路由规则' : '新建路由规则'} onClose={onClose} onSave={() => model.save({ ...config, auto_rules: config.auto_rules.some(item => item.id === rule.id) ? config.auto_rules.map(item => item.id === rule.id ? result() : item) : [...config.auto_rules, result()] })}>
    <TextInput label="规则名称" value={rule.name} onChange={name => set({ name })} isRequired />
    <Grid columns={{ minWidth: 240 }} gap={4}>
      <TextInput label="进程名称" value={rule.process_name} onChange={process_name => set({ process_name })} description="支持 * 和 ?，例如 chrome.exe" />
      <TextInput label="程序路径前缀" value={rule.image_path_pattern} onChange={image_path_pattern => set({ image_path_pattern })} description="例如 C:\Apps\，忽略大小写" />
    </Grid>
    <TextInput label="命令行条件" value={rule.cmdline_pattern} onChange={cmdline_pattern => set({ cmdline_pattern })} description="空格分隔的词需同时命中；包含 * 或 ? 时按完整通配模式匹配。" />
    <Text type="supporting">至少填写一个匹配条件；已填写的条件需同时匹配。规则会被后代进程继承。</Text>
    <Grid columns={{ minWidth: 240 }} gap={4}>
      <GroupSelector groups={config.proxy_groups} value={rule.proxy_group_id} onChange={proxy_group_id => set({ proxy_group_id })} />
      <Selector label="协议" value={rule.protocol} onChange={protocol => set({ protocol: protocol as AutoRule['protocol'] })} options={[{ value: 'both', label: 'TCP 与 UDP' }, { value: 'tcp', label: 'TCP' }, { value: 'udp', label: 'UDP' }]} width="100%" />
      {(['include_cidrs', 'exclude_cidrs', 'include_ports', 'exclude_ports'] as const).map((key, index) => <TextArea key={key} label={['包含目标网段', '排除目标网段', '包含目标端口', '排除目标端口'][index]} value={filters[key]} onChange={value => setFilters({ ...filters, [key]: value })} rows={2} description={index < 2 ? '每行一个 IPv4 / CIDR，留空不限制。' : '每行一个端口或范围，例如 443、8000-9000。'} />)}
    </Grid>
    <CheckboxInput label="启用这条规则" value={rule.enabled} onChange={enabled => set({ enabled })} description="按列表顺序匹配，第一条命中的规则生效；排除条件优先。" />
  </Editor>;
}
export default function Rules({ config }: { config: Config }) {
  const model = useAppModel(); const action = useAction();
  const [editing, setEditing] = useState<{ config: Config; rule: AutoRule }>(); const [deleting, setDeleting] = useState<AutoRule>();
  const move = async (id: string, offset: number) => { const rules = [...config.auto_rules]; const index = rules.findIndex(item => item.id === id); [rules[index], rules[index + offset]] = [rules[index + offset], rules[index]]; await model.save({ ...config, auto_rules: rules }); };
  const columns: TableColumn<AutoRule>[] = [
    { key: 'name', header: '规则', width: proportional(2), renderCell: rule => <VStack gap={1}><Text weight="semibold">{rule.name}</Text><Token label={rule.enabled ? '已启用' : '已停用'} color={rule.enabled ? 'green' : 'gray'} size="sm" /></VStack> },
    { key: 'match', header: '匹配条件', width: proportional(3), renderCell: rule => <Text type="code" maxLines={2}>{[rule.process_name, rule.image_path_pattern, rule.cmdline_pattern].filter(Boolean).join(' · ')}</Text> },
    { key: 'protocol', header: '协议', renderCell: rule => <Text>{rule.protocol === 'both' ? 'TCP / UDP' : rule.protocol.toUpperCase()}</Text> },
    { key: 'proxy_group_id', header: '代理组', renderCell: rule => <Text>{config.proxy_groups.find(group => group.id === rule.proxy_group_id)?.name}</Text> },
    { key: 'actions', header: '操作 / 优先级', width: proportional(3), renderCell: rule => <HStack gap={1} wrap="wrap"><Button label="编辑" size="sm" onClick={() => setEditing({ config, rule })} /><Button label={rule.enabled ? '停用' : '启用'} size="sm" isDisabled={action.busy} onClick={() => void action.run(() => model.save({ ...config, auto_rules: config.auto_rules.map(item => item.id === rule.id ? { ...item, enabled: !item.enabled } : item) }))} /><Button label="上移" size="sm" variant="ghost" isDisabled={action.busy || config.auto_rules[0].id === rule.id} onClick={() => void action.run(() => move(rule.id, -1))} /><Button label="下移" size="sm" variant="ghost" isDisabled={action.busy || config.auto_rules.at(-1)?.id === rule.id} onClick={() => void action.run(() => move(rule.id, 1))} /><Button label="删除" size="sm" variant="ghost" onClick={() => setDeleting(rule)} /></HStack> },
  ];
  return <><Layout header={<LayoutHeader hasDivider><HStack vAlign="center" hAlign="between" gap={3} wrap="wrap"><VStack gap={1}><Heading level={1}>路由规则</Heading><Text color="secondary">从上到下匹配，手动进程选择优先。</Text></VStack><Button label="新建规则" variant="primary" onClick={() => setEditing({ config, rule: { id: crypto.randomUUID(), name: '', enabled: false, process_name: '', image_path_pattern: '', cmdline_pattern: '', proxy_group_id: config.proxy_groups[0].id, protocol: 'both', dst_filter: { include_cidrs: [], exclude_cidrs: [], include_ports: [], exclude_ports: [] } } })} /></HStack></LayoutHeader>}
    content={<LayoutContent><VStack gap={4}>{action.error && <Banner status="error" title="规则更新失败" description={action.error} />}{config.auto_rules.length ? <Table data={config.auto_rules} columns={columns} idKey="id" hasHover /> : <EmptyState title="还没有自动规则" description="添加进程匹配条件，并选择它使用的代理组。未匹配的进程保持直连。" />}</VStack></LayoutContent>} />
    {editing && <RuleEditor config={editing.config} initial={editing.rule} onClose={() => setEditing(undefined)} />}
    {deleting && <Editor title="删除规则" saveLabel="删除规则" destructive onClose={() => setDeleting(undefined)} onSave={() => model.save({ ...config, auto_rules: config.auto_rules.filter(item => item.id !== deleting.id) })}><Text>删除「{deleting.name}」后，新的连接将使用其余匹配规则。</Text></Editor>}
  </>;
}
