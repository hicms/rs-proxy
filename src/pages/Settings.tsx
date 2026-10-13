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
import { GroupSelector } from '../components/Editor';
import { useAction, useAppModel, lines } from '../model';
import type { Config } from '../bindings/Config';
import type { AutostartState } from '../bindings/AutostartState';

function Section({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return <Grid columns={{ minWidth: 320, max: 2 }} gap={10}><VStack gap={1}><Heading level={2}>{title}</Heading><Text type="supporting" color="secondary">{description}</Text></VStack><VStack gap={4}>{children}</VStack></Grid>;
}
function Autostart() {
  const [state, setState] = useState<AutostartState>(); const action = useAction();
  useEffect(() => { void action.run(async () => setState(await invoke<AutostartState>('get_autostart'))); }, []);
  return <VStack gap={4}>{action.error && <Banner status="error" title="启动设置未完成" description={action.error} />}
    {!state ? <Button label="读取启动设置" isLoading={action.busy} onClick={() => void action.run(async () => setState(await invoke<AutostartState>('get_autostart')))} /> : <>
      <CheckboxInput label="登录 Windows 时启动" value={state.enabled} onChange={enabled => setState({ ...state, enabled })} />
      <CheckboxInput label="启动后收起到托盘" value={state.start_minimized} onChange={start_minimized => setState({ ...state, start_minimized })} />
      <HStack><Button label="应用启动设置" isLoading={action.busy} onClick={() => void action.run(async () => setState(await invoke<AutostartState>('set_autostart', { enabled: state.enabled, startMinimized: state.start_minimized })))} /></HStack>
    </>}
  </VStack>;
}
export default function Settings({ config }: { config: Config }) {
  const [draft, setDraft] = useState(config); const [excluded, setExcluded] = useState(config.default_exclude_cidrs.join('\n')); const [notice, setNotice] = useState('');
  const model = useAppModel(); const action = useAction();
  const save = async () => { await model.save({ ...draft, default_exclude_cidrs: lines(excluded) }); };
  const setDns = (patch: Partial<Config['dns']>) => setDraft(previous => ({ ...previous, dns: { ...previous.dns, ...patch } }));
  return <Layout height="fill" contentWidth={1440} header={<LayoutHeader hasDivider><HStack vAlign="center" hAlign="between"><VStack gap={1}><Heading level={1}>设置</Heading><Text color="secondary">外观、网络和桌面行为。</Text></VStack><Text type="supporting">配置版本 {config.revision}</Text></HStack></LayoutHeader>}
    content={<LayoutContent padding={4}><VStack gap={4}>
      {action.error && <Banner status="error" title="设置未保存" description={action.error} />}
      {notice && <Banner status="success" title={notice} isDismissable onDismiss={() => setNotice('')} />}
      <Section title="外观与窗口" description="选择适合你的明暗外观。关闭到托盘后，引擎会继续运行。">
        <Selector label="主题" value={draft.ui.theme} onChange={theme => setDraft({ ...draft, ui: { ...draft.ui, theme } })} options={[{ value: 'system', label: '跟随系统' }, { value: 'light', label: '浅色' }, { value: 'dark', label: '深色' }]} width="100%" />
        <CheckboxInput label="关闭窗口时收起到托盘" value={draft.ui.close_to_tray} onChange={close_to_tray => setDraft({ ...draft, ui: { ...draft.ui, close_to_tray } })} />
      </Section><Divider />
      <Section title="DNS 转发" description="启用后，活动 IPv4 网络接口的 DNS 会指向本地监听器，经指定代理转发。停止引擎或正常退出时恢复原设置。">
        <CheckboxInput label="启用 DNS 转发" value={draft.dns.enabled} onChange={enabled => setDns({ enabled })} />
        <GroupSelector groups={draft.proxy_groups} value={draft.dns.proxy_group_id} onChange={proxy_group_id => setDns({ proxy_group_id })} />
        <TextInput label="上游 DNS IPv4 地址" value={draft.dns.upstream_host} onChange={upstream_host => setDns({ upstream_host })} />
        <NumberInput label="上游 DNS 端口" value={draft.dns.upstream_port} onChange={upstream_port => setDns({ upstream_port })} min={1} max={65535} isIntegerOnly isWheelEnabled={false} />
        <TextInput label="本地 DNS 地址" value={draft.dns.listen_host} onChange={listen_host => setDns({ listen_host })} description="必须为回环 IPv4 地址，监听端口固定为 53。" />
      </Section><Divider />
      <Section title="直连网段" description="所有代理规则都会排除以下 IPv4 网段。每行一个地址或 CIDR。">
        <TextArea label="全局排除网段" value={excluded} onChange={setExcluded} rows={6} />
      </Section><Divider />
      <Section title="首包等待与日志" description="等待进程归属确认后处理 TCP 首包；超时则保持该连接直连。日志级别在下次启动后生效。">
        <NumberInput label="首包等待时间" value={draft.tcp_syn_parking.watchdog_ms} onChange={watchdog_ms => setDraft({ ...draft, tcp_syn_parking: { ...draft.tcp_syn_parking, watchdog_ms } })} min={5} max={50} units="ms" isIntegerOnly isWheelEnabled={false} />
        <NumberInput label="暂存队列容量" value={draft.tcp_syn_parking.pool_size} onChange={pool_size => setDraft({ ...draft, tcp_syn_parking: { ...draft.tcp_syn_parking, pool_size } })} min={32} max={4096} isIntegerOnly isWheelEnabled={false} />
        <Selector label="日志级别" value={draft.log_level} onChange={log_level => setDraft({ ...draft, log_level })} options={['error', 'warn', 'info', 'debug', 'trace']} width="100%" />
      </Section><Divider />
      <Section title="自动启动" description="为当前 Windows 用户创建具有管理员权限的登录任务。此处单独应用。"><Autostart /></Section><Divider />
      <Section title="配置文件" description="导入会校验并替换当前保存的配置；导出包含所有代理组、规则和设置。">
        <HStack gap={2} wrap="wrap"><Button label="导入配置" isDisabled={action.busy} onClick={() => void action.run(async () => { const imported = await invoke<Config | null>('import_config', { revision: config.revision }); if (imported) await model.refresh(); })} /><Button label="导出配置" isDisabled={action.busy} onClick={() => void action.run(async () => { if (await invoke<boolean>('export_config')) setNotice('配置已导出'); })} /></HStack>
      </Section>
    </VStack></LayoutContent>}
    footer={<LayoutFooter hasDivider><HStack vAlign="center" hAlign="between" gap={3} wrap="wrap"><Text type="supporting">IPv4 · SOCKS5 · RS Proxy 0.1.0</Text><HStack gap={2}><Button label="放弃修改" isDisabled={action.busy} onClick={() => { setDraft(config); setExcluded(config.default_exclude_cidrs.join('\n')); }} /><Button label="保存设置" variant="primary" isLoading={action.busy} onClick={() => void action.run(save)} /></HStack></HStack></LayoutFooter>} />;
}
