import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AppShell } from '@astryxdesign/core/AppShell';
import { TopNav, TopNavHeading, TopNavItem } from '@astryxdesign/core/TopNav';
import { HStack, Layout, LayoutContent } from '@astryxdesign/core/Layout';
import { Button } from '@astryxdesign/core/Button';
import { Banner } from '@astryxdesign/core/Banner';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { StatusDot } from '@astryxdesign/core/StatusDot';
import { Text } from '@astryxdesign/core/Text';
import { Icon } from '@astryxdesign/core/Icon';
import { Activity, Route, Server, Settings2, Waypoints, Power, Unplug } from 'lucide-react';
import { Theme } from '@astryxdesign/core/theme';
import { themes, defaultThemeSlug } from './astryx-themes';
import { ModelContext, useModel, useAction } from './model';
import Processes from './pages/Processes';
import Rules from './pages/Rules';
import Proxies from './pages/Proxies';
import Settings from './pages/Settings';

const pages = { processes: '进程与连接', rules: '路由规则', proxies: '代理组', settings: '设置' };
const pageIcons = { processes: Activity, rules: Route, proxies: Server, settings: Settings2 };
type Page = keyof typeof pages;
const currentPage = (): Page => { const key = location.hash.slice(1); return key in pages ? key as Page : 'processes'; };
export function App() {
  const model = useModel(); const action = useAction();
  const [page, setPage] = useState<Page>(currentPage);
  useEffect(() => { const changed = () => setPage(currentPage()); window.addEventListener('hashchange', changed); return () => window.removeEventListener('hashchange', changed); }, []);
  const snapshot = model.snapshot;
  const running = snapshot?.stats.running ?? false;
  const error = action.error || model.error || snapshot?.stats.last_error;
  const mode = snapshot?.config.ui.theme as 'system' | 'light' | 'dark' | undefined;
  return <ModelContext.Provider value={model}><Theme theme={themes[defaultThemeSlug]} mode={mode ?? 'system'}>
    <AppShell height="fill" contentPadding={0} variant="section"
      banner={error ? <Banner status="error" title={snapshot ? '需要处理' : '尚未连接'} description={error} container="section" /> : undefined}
      topNav={<TopNav label="主导航" heading={<TopNavHeading heading="RS Proxy" logo={<Icon icon={Waypoints} size="lg" />} />}
        startContent={Object.entries(pages).map(([key, label]) => <TopNavItem key={key} href={`#${key}`} label={label} icon={<Icon icon={pageIcons[key as Page]} />} isSelected={page === key} />)}
        endContent={<HStack gap={3} vAlign="center"><StatusDot variant={running ? 'success' : 'neutral'} label={running ? '引擎运行中' : '引擎未运行'} /><Text type="supporting">{running ? '运行中' : '已停止'}</Text>
          <Button label={running ? '暂停代理' : '启动代理'} icon={<Icon icon={Power} />} variant={running ? 'ghost' : 'primary'} isDisabled={!snapshot} isLoading={action.busy} onClick={() => void action.run(async () => { await invoke('set_engine_running', { running: !running }); await model.refresh(); })} />
        </HStack>} />}>
      {!snapshot ? <Layout content={<LayoutContent><EmptyState icon={<Icon icon={Unplug} size="lg" />} headingLevel={1} title={model.error ? '无法读取桌面服务' : '正在连接桌面服务'} description={model.error || '正在读取进程、连接和配置。'} actions={<Button label="重试连接" onClick={() => void model.refresh()} />} /></LayoutContent>} /> :
        page === 'processes' ? <Processes snapshot={snapshot} /> : page === 'rules' ? <Rules config={snapshot.config} /> : page === 'proxies' ? <Proxies config={snapshot.config} /> : <Settings config={snapshot.config} />}
    </AppShell>
  </Theme></ModelContext.Provider>;
}
