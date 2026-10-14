import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import type { Config } from './bindings/Config';
import type { Snapshot } from './bindings/Snapshot';
import type { AutostartState } from './bindings/AutostartState';

export function useModel() {
  const [snapshot, setSnapshot] = useState<Snapshot>();
  const [error, setError] = useState('');
  const [pendingChanges, setPendingChanges] = useState(0);
  const [changingAutostart, setChangingAutostart] = useState(false);
  const autostartActive = useRef(false);
  const setAutostart = useCallback(async (state: AutostartState) => {
    if (autostartActive.current) throw new Error('启动设置正在保存');
    autostartActive.current = true; setChangingAutostart(true);
    try { return await invoke<AutostartState>('set_autostart', { enabled: state.enabled, startMinimized: state.start_minimized }); }
    finally { autostartActive.current = false; setChangingAutostart(false); }
  }, []);
  const loading = useRef(false);
  const refresh = useCallback(async () => {
    if (loading.current) return;
    if (!isTauri()) { setError('请在 RS Proxy 桌面应用中连接代理引擎。'); return; }
    loading.current = true;
    try {
      const next = await invoke<Snapshot>('get_snapshot');
      setSnapshot(previous => previous?.config.revision === next.config.revision ? { ...next, config: previous.config } : next);
      setError('');
    }
    catch (error) { setError(String(error)); }
    finally { loading.current = false; }
  }, []);
  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => { if (!document.hidden) void refresh(); }, 1000);
    const listener = isTauri() ? listen<string>('service-error', event => setError(event.payload)) : undefined;
    return () => { clearInterval(timer); void listener?.then(unlisten => unlisten()); };
  }, [refresh]);
  const changeConfig = useCallback(async <T>(action: () => Promise<T>): Promise<T> => {
    setPendingChanges(count => count + 1);
    try { return await action(); }
    finally { setPendingChanges(count => count - 1); }
  }, []);
  const save = useCallback((config: Config) => changeConfig(async () => {
    const saved = await invoke<Config>('save_config', { config });
    setSnapshot(previous => previous && ({ ...previous, config: saved }));
    await refresh();
  }), [refresh, changeConfig]);
  return { snapshot, error, refresh, save, changeConfig, changingConfig: pendingChanges > 0, setAutostart, changingAutostart };
}
export type Model = ReturnType<typeof useModel>;
export type ModelActions = Omit<Model, 'snapshot' | 'error'>;
export const ModelContext = createContext<ModelActions | null>(null);
export function useAppModel() {
  const model = useContext(ModelContext);
  if (!model) throw new Error('缺少应用状态');
  return model;
}
export function useAction() {
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const active = useRef(false);
  const run = async (action: () => Promise<unknown>) => {
    if (active.current) return false;
    active.current = true; setBusy(true); setError('');
    try { await action(); return true; }
    catch (error) { setError(String(error)); return false; }
    finally { active.current = false; setBusy(false); }
  };
  return { error, busy, run };
}
export const lines = (value: string) => value.split(/[\n,]/).map(item => item.trim()).filter(Boolean);
export function bytes(value: number) {
  if (value < 1024) return `${Math.round(value)} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KiB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MiB`;
  return `${(value / 1024 ** 3).toFixed(1)} GiB`;
}
