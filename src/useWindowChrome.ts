import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

type WindowAction = 'minimize' | 'maximize' | 'close';

export function useWindowChrome() {
  const [custom, setCustom] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const [error, setError] = useState('');
  const pending = useRef(false);

  useEffect(() => {
    if (!isTauri()) return;
    const nativeWindow = getCurrentWindow();
    let stopped = false;
    let generation = 0;
    let unlisten: (() => void) | undefined;
    const report = (error: unknown) => { if (!stopped) setError(String(error)); };
    const sync = async () => {
      const request = ++generation;
      try {
        const value = await nativeWindow.isMaximized();
        if (!stopped && request === generation) setMaximized(value);
      } catch (error) { report(error); }
    };
    void (async () => {
      if (await nativeWindow.isDecorated() || stopped) return;
      setCustom(true);
      const dispose = await nativeWindow.onResized(() => void sync());
      if (stopped) { dispose(); return; }
      unlisten = dispose;
      await sync();
    })().catch(report);
    return () => { stopped = true; unlisten?.(); };
  }, []);

  async function run(action: WindowAction) {
    if (pending.current) return;
    pending.current = true;
    setError('');
    try {
      const nativeWindow = getCurrentWindow();
      if (action === 'minimize') await nativeWindow.minimize();
      else if (action === 'close') await nativeWindow.close();
      else {
        await nativeWindow.toggleMaximize();
        setMaximized(await nativeWindow.isMaximized());
      }
    } catch (error) { setError(String(error)); }
    finally { pending.current = false; }
  }

  return { custom, maximized, error, run };
}
