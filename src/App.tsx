import { memo, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AppShell } from '@astryxdesign/core/AppShell';
import { TopNav, TopNavHeading } from '@astryxdesign/core/TopNav';
import { SideNav, SideNavItem, SideNavSection } from '@astryxdesign/core/SideNav';
import { HStack, VStack, Layout, LayoutContent } from '@astryxdesign/core/Layout';
import { Button } from '@astryxdesign/core/Button';
import { IconButton } from '@astryxdesign/core/IconButton';
import { Banner } from '@astryxdesign/core/Banner';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { StatusDot } from '@astryxdesign/core/StatusDot';
import { Icon } from '@astryxdesign/core/Icon';
import { Activity, Route, Server, Settings2, Waypoints, Power, Unplug } from 'lucide-react';
import { Theme } from '@astryxdesign/core/theme';
import { themes, defaultThemeSlug } from './astryx-themes';
import { ModelContext, useModel, useAction } from './model';
import { useWindowChrome } from './useWindowChrome';
import { WindowControls } from './components/WindowControls';
import { Accent } from './components/PageHeading';
import { RetainedView } from './components/RetainedView';
import Processes from './pages/Processes';
import Rules from './pages/Rules';
import Proxies from './pages/Proxies';
import Settings from './pages/Settings';

const pages = { processes: '进程与连接', rules: '路由规则', proxies: '代理组', settings: '设置' };
const pageIcons = { processes: Activity, rules: Route, proxies: Server, settings: Settings2 };
type Page = keyof typeof pages;
const currentPage = (): Page => { const key = location.hash.slice(1); return key in pages ? key as Page : 'processes'; };
const ProcessesPage = memo(Processes);
const RulesPage = memo(Rules);
const ProxiesPage = memo(Proxies);
const SettingsPage = memo(Settings);

export function App() {
  const model = useModel(); const action = useAction();
  const chrome = useWindowChrome();
  const [page, setPage] = useState<Page>(currentPage);
  const [visited, setVisited] = useState<Set<Page>>(() => new Set([currentPage()]));
  useEffect(() => {
    const changed = () => { const next = currentPage(); setPage(next); setVisited(previous => previous.has(next) ? previous : new Set([...previous, next])); };
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, []);
  const { refresh, save, changeConfig, changingConfig, setAutostart, changingAutostart } = model;
  const actions = useMemo(() => ({ refresh, save, changeConfig, changingConfig, setAutostart, changingAutostart }), [refresh, save, changeConfig, changingConfig, setAutostart, changingAutostart]);
  const snapshot = model.snapshot;
  const running = snapshot?.stats.running ?? false;
  const error = chrome.error || action.error || model.error || snapshot?.stats.last_error;
  const mode = snapshot?.config.ui.theme as 'system' | 'light' | 'dark' | undefined;
  return <ModelContext.Provider value={actions}><Theme theme={themes[defaultThemeSlug]} mode={mode ?? 'system'}>
    <AppShell height="fill" contentPadding={0} variant="section" className="rs-proxy-shell" data-custom-titlebar={chrome.custom}
      topNav={<TopNav label="RS Proxy 窗口" className={chrome.custom ? 'window-titlebar' : undefined} data-tauri-drag-region={chrome.custom ? 'deep' : undefined}
        heading={<TopNavHeading heading="RS Proxy" logo={<Accent><Icon icon={Waypoints} size="sm" color="accent" /></Accent>} />}
        endContent={<HStack gap={3} vAlign="center" data-tauri-drag-region="false"><WindowControls chrome={chrome} /></HStack>} />}
      sideNav={<VStack padding={2} height="100%"><SideNav aria-label="主导航" collapsible={{ defaultIsCollapsed: false, hasButton: true }} resizable={{ defaultWidth: 184, minWidth: 160, maxWidth: 240 }}
        header={<VStack padding={4} hAlign="center"><Accent><Icon icon={Waypoints} size="md" color="accent" /></Accent></VStack>}
        footer={<VStack gap={4} padding={3} hAlign="center"><StatusDot variant={running ? 'success' : 'neutral'} label={running ? '引擎运行中' : '引擎未运行'} tooltip={running ? '运行中' : '已停止'} />
          <IconButton label={running ? '暂停代理' : '启动代理'} tooltip={running ? '暂停代理' : '启动代理'} icon={<Icon icon={Power} />} variant="ghost" isDisabled={!snapshot || action.busy} onClick={() => void action.run(async () => { await invoke('set_engine_running', { running: !running }); await model.refresh(); })} />
        </VStack>}>
        <Accent><SideNavSection title="主导航" isHeaderHidden>{Object.entries(pages).map(([key, label]) => <SideNavItem key={key} href={`#${key}`} label={label} icon={<Icon icon={pageIcons[key as Page]} />} isSelected={page === key} />)}</SideNavSection></Accent>
      </SideNav></VStack>}>
      {error ? <Banner status="error" title={snapshot ? '需要处理' : '尚未连接'} description={error} container="section" /> : null}
      {!snapshot ? <Layout content={<LayoutContent><EmptyState icon={<Icon icon={Unplug} size="lg" />} headingLevel={1} title={model.error ? '无法读取桌面服务' : '正在连接桌面服务'} description={model.error || '正在读取进程、连接和配置。'} actions={<Button label="重试连接" onClick={() => void model.refresh()} />} /></LayoutContent>} /> : <>
        {visited.has('processes') && <RetainedView visible={page === 'processes'}><ProcessesPage snapshot={snapshot} /></RetainedView>}
        {visited.has('rules') && <RetainedView visible={page === 'rules'}><RulesPage config={snapshot.config} /></RetainedView>}
        {visited.has('proxies') && <RetainedView visible={page === 'proxies'}><ProxiesPage config={snapshot.config} /></RetainedView>}
        {visited.has('settings') && <RetainedView visible={page === 'settings'}><SettingsPage config={snapshot.config} /></RetainedView>}
      </>}
    </AppShell>
  </Theme></ModelContext.Provider>;
}
