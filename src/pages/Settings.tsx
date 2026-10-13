// Copyright (c) Meta Platforms, Inc. and affiliates.
import { useEffect, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Layout, LayoutHeader, LayoutContent, LayoutFooter, VStack, HStack } from '@astryxdesign/core/Layout';
import { Heading, Text } from '@astryxdesign/core/Text';
import { Grid } from '@astryxdesign/core/Grid';
import { Divider } from '@astryxdesign/core/Divider';
import { CheckboxInput } from '@astryxdesign/core/CheckboxInput';
import { Selector } from '@astryxdesign/core/Selector';
import { TextInput } from '@astryxdesign/core/TextInput';
import { TextArea } from '@astryxdesign/core/TextArea';
import { NumberInput } from '@astryxdesign/core/NumberInput';
import { Button } from '@astryxdesign/core/Button';
import { Banner } from '@astryxdesign/core/Banner';
import { Icon } from '@astryxdesign/core/Icon';
import { TabList, Tab } from '@astryxdesign/core/TabList';
import { Palette, Network, Route, SlidersHorizontal, Power, FolderOpen, Download, Upload, Save, Undo2 } from 'lucide-react';
import { GroupSelector } from '../components/Editor';
import { useAction, useAppModel, lines } from '../model';
import type { Config } from '../bindings/Config';
import type { AutostartState } from '../bindings/AutostartState';

function Section({ title, description, icon, children }: { title: string; description: string; icon: ReactNode; children: ReactNode }) {
  return <Grid columns={{ minWidth: 320, max: 2 }} gap={10}>
    <VStack gap={2}><HStack gap={2} vAlign="center">{icon}<Heading level={2}>{title}</Heading></HStack><Text type="supporting" color="secondary">{description}</Text></VStack>
    <VStack gap={4}>{children}</VStack>
  </Grid>;
}

function Autostart() {
  const model = useAppModel();
  const [state, setState] = useState<AutostartState>();
  const [saved, setSaved] = useState<AutostartState>();
  const action = useAction();
  const busy = action.busy || model.changingAutostart;
  const read = async () => { const value = await invoke<AutostartState>('get_autostart'); setState(value); setSaved(value); };
  useEffect(() => { if (!model.changingAutostart) void action.run(read); }, [model.changingAutostart]);
  const dirty = state?.enabled !== saved?.enabled || state?.start_minimized !== saved?.start_minimized;
  return <VStack gap={4}>
    {action.error && <Banner status="error" title="启动设置未完成" description={action.error} />}
    {!state ? <Button label="读取启动设置" isLoading={busy} onClick={() => void action.run(read)} /> : <>
      <CheckboxInput label="登录 Windows 时启动" value={state.enabled} isDisabled={busy} onChange={enabled => setState({ ...state, enabled })} />
      <CheckboxInput label="启动后收起到托盘" value={state.start_minimized} isDisabled={busy} onChange={start_minimized => setState({ ...state, start_minimized })} />
      <HStack><Button label="应用启动设置" icon={<Icon icon={Power} />} isDisabled={!dirty} isLoading={busy} onClick={() => void action.run(async () => {
        const value = await model.setAutostart(state); setState(value); setSaved(value);
      })} /></HStack>
    </>}
  </VStack>;
}

export default function Settings({ config }: { config: Config }) {
  const [draft, setDraft] = useState(config);
  const [excluded, setExcluded] = useState(config.default_exclude_cidrs.join('\n'));
  const [notice, setNotice] = useState('');
  const [page, setPage] = useState('general');
  const model = useAppModel(); const action = useAction();
  const busy = action.busy || model.changingConfig;
  useEffect(() => { setDraft(config); setExcluded(config.default_exclude_cidrs.join('\n')); }, [config.revision]);
  const next = { ...draft, default_exclude_cidrs: lines(excluded) };
  const dirty = JSON.stringify(next) !== JSON.stringify(config);
  const save = async () => { await model.save(next); setNotice('设置已保存'); };
  const setDns = (patch: Partial<Config['dns']>) => setDraft(previous => ({ ...previous, dns: { ...previous.dns, ...patch } }));
  return <Layout height="fill" contentWidth={1440}
    header={<LayoutHeader hasDivider><VStack gap={4}>
      <Heading level={1}>设置</Heading>
      <TabList value={page} onChange={setPage}>
        <Tab value="general" label="常规" icon={<Icon icon={Palette} />} />
        <Tab value="network" label="网络" icon={<Icon icon={Network} />} />
        <Tab value="advanced" label="高级" icon={<Icon icon={SlidersHorizontal} />} />
      </TabList>
    </VStack></LayoutHeader>}
    content={<LayoutContent padding={4}><VStack gap={6}>
      {action.error && <Banner status="error" title="设置未保存" description={action.error} />}
      {notice && !dirty && <Banner status="success" title={notice} isDismissable onDismiss={() => setNotice('')} />}
      {page === 'general' && <>
        <Section icon={<Icon icon={Palette} color="secondary" />} title="外观与窗口" description="选择明暗外观，按你的习惯工作。">
          <Selector isDisabled={busy} label="主题" value={draft.ui.theme} onChange={theme => setDraft({ ...draft, ui: { ...draft.ui, theme } })} options={[{ value: 'system', label: '跟随系统' }, { value: 'light', label: '浅色' }, { value: 'dark', label: '深色' }]} width="100%" />
          <CheckboxInput isDisabled={busy} label="关闭窗口时收起到托盘" description="收起后，代理继续运行。" value={draft.ui.close_to_tray} onChange={close_to_tray => setDraft({ ...draft, ui: { ...draft.ui, close_to_tray } })} />
        </Section><Divider />
        <Section icon={<Icon icon={Power} color="secondary" />} title="自动启动" description="登录时准备好代理。此设置单独应用。"><Autostart /></Section><Divider />
        <Section icon={<Icon icon={FolderOpen} color="secondary" />} title="配置文件" description="备份设置，或载入一份已有配置。">
          <HStack gap={2} wrap="wrap">
            <Button label="导入配置" icon={<Icon icon={Upload} />} isDisabled={busy || dirty} tooltip={dirty ? '请先保存或放弃当前修改' : undefined} onClick={() => void action.run(() => model.changeConfig(async () => { const imported = await invoke<Config | null>('import_config', { revision: config.revision }); if (imported) { await model.refresh(); setNotice('配置已导入'); } }))} />
            <Button label="导出配置" icon={<Icon icon={Download} />} isDisabled={busy} onClick={() => void action.run(async () => { if (await invoke<boolean>('export_config')) setNotice('已导出保存的配置'); })} />
          </HStack>
        </Section>
      </>}
      {page === 'network' && <>
        <Section icon={<Icon icon={Network} color="secondary" />} title="进程 DNS" description="已选进程的普通 DNS 查询跟随其代理组，不修改系统 DNS。">
          <CheckboxInput isDisabled={busy} label="代理进程的 DNS" description="DNS 查询不受全局直连网段限制，仍遵守路由规则。需要内网域名时可关闭。" value={draft.proxy_process_dns} onChange={proxy_process_dns => setDraft(previous => ({ ...previous, proxy_process_dns }))} />
          <TextInput isDisabled={busy} label="上游 DNS IPv4 地址" value={draft.dns.upstream_host} onChange={upstream_host => setDns({ upstream_host })} />
          <NumberInput isDisabled={busy} label="上游 DNS 端口" value={draft.dns.upstream_port} onChange={upstream_port => setDns({ upstream_port })} min={1} max={65535} isIntegerOnly isWheelEnabled={false} />
        </Section><Divider />
        <Section icon={<Icon icon={Network} color="secondary" />} title="系统 DNS 转发" description="使用同一上游 DNS。启用后更改系统 DNS，停止代理或正常退出时恢复。">
          <CheckboxInput isDisabled={busy} label="启用 DNS 转发" value={draft.dns.enabled} onChange={enabled => setDns({ enabled })} />
          <GroupSelector groups={draft.proxy_groups} value={draft.dns.proxy_group_id} isDisabled={busy} onChange={proxy_group_id => setDns({ proxy_group_id })} />
          <TextInput isDisabled={busy} label="本地 DNS 地址" value={draft.dns.listen_host} onChange={listen_host => setDns({ listen_host })} description="回环 IPv4 地址，监听端口为 53。" />
        </Section><Divider />
        <Section icon={<Icon icon={Route} color="secondary" />} title="直连网段" description="这些地址默认直连，已启用的进程 DNS 转发除外。每行一个 IPv4 地址或 CIDR。">
          <TextArea isDisabled={busy} label="全局排除网段" value={excluded} onChange={setExcluded} rows={5} />
        </Section>
      </>}
      {page === 'advanced' && <Section icon={<Icon icon={SlidersHorizontal} color="secondary" />} title="连接与诊断" description="通常保持默认即可。日志级别在下次启动后生效。">
        <NumberInput isDisabled={busy} label="首包等待时间" description="等待确认进程归属；超时后该连接直连。" value={draft.tcp_syn_parking.watchdog_ms} onChange={watchdog_ms => setDraft({ ...draft, tcp_syn_parking: { ...draft.tcp_syn_parking, watchdog_ms } })} min={5} max={50} units="ms" isIntegerOnly isWheelEnabled={false} />
        <NumberInput isDisabled={busy} label="暂存队列容量" value={draft.tcp_syn_parking.pool_size} onChange={pool_size => setDraft({ ...draft, tcp_syn_parking: { ...draft.tcp_syn_parking, pool_size } })} min={32} max={4096} isIntegerOnly isWheelEnabled={false} />
        <Selector isDisabled={busy} label="日志级别" value={draft.log_level} onChange={log_level => setDraft({ ...draft, log_level })} options={['error', 'warn', 'info', 'debug', 'trace']} width="100%" />
      </Section>}
    </VStack></LayoutContent>}
    footer={<LayoutFooter hasDivider><HStack vAlign="center" hAlign="between" gap={3} wrap="wrap">
      <Text type="supporting" color="secondary">{dirty ? '有未保存的修改' : '所有设置已保存'}</Text>
      <HStack gap={2}><Button label="放弃修改" variant="ghost" icon={<Icon icon={Undo2} />} isDisabled={busy || !dirty} onClick={() => { setDraft(config); setExcluded(config.default_exclude_cidrs.join('\n')); setNotice(''); }} /><Button label="保存设置" icon={<Icon icon={Save} />} variant="primary" isDisabled={!dirty} isLoading={busy} onClick={() => void action.run(save)} /></HStack>
    </HStack></LayoutFooter>} />;
}
