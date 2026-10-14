import { invoke } from '@tauri-apps/api/core';
import { useEffect, useRef, useState } from 'react';
import type { ProcessIcon } from './bindings/ProcessIcon';
import type { ProcessIdentity } from './bindings/ProcessIdentity';
import type { ProcessView } from './bindings/ProcessView';

export const processKey = (identity: ProcessIdentity) => `${identity.pid}:${identity.sequence}`;
type Job = { identities: ProcessIdentity[]; cancelled: boolean; receive: (icons: ProcessIcon[]) => void };
let running = false;
let pending: Job | undefined;

// Shared across page mounts: finishing an obsolete native call precedes the latest page.
async function drain() {
  if (running || !pending) return;
  const job = pending; pending = undefined;
  if (job.cancelled) return;
  running = true;
  try {
    const icons = await invoke<ProcessIcon[]>('get_process_icons', { identities: job.identities });
    if (!job.cancelled) job.receive(icons);
  } catch (error) {
    console.warn('程序图标读取失败', error);
    if (!job.cancelled) job.receive(job.identities.map(identity => ({ identity, data_url: null })));
  } finally {
    running = false;
    void drain();
  }
}

export function useProcessIcons(processes: ProcessView[], active = true) {
  const signature = JSON.stringify(processes.map(({ process }) => [process.identity, process.image_path]));
  const [result, setResult] = useState<{ signature: string; icons: Map<string, string | null> }>();
  const cached = useRef(result);
  useEffect(() => {
    if (!active) return;
    const identities = (JSON.parse(signature) as [ProcessIdentity, string][]).map(([identity]) => identity);
    if (!identities.length) return;
    let job: Job | undefined;
    const request = () => {
      // A slow native call cannot be cancelled; let it settle before retrying.
      if (job && !job.cancelled) return;
      job = { identities, cancelled: false, receive: icons => {
        if (job) job.cancelled = true;
        cached.current = { signature, icons: new Map(icons.map(icon => [processKey(icon.identity), icon.data_url])) };
        setResult(cached.current);
      } };
      pending = job;
      queueMicrotask(() => { void drain(); });
    };
    if (cached.current?.signature !== signature) request();
    const timer = window.setInterval(request, 30000);
    return () => { clearInterval(timer); if (job) job.cancelled = true; };
  }, [signature, active]);
  return result?.signature === signature ? result.icons : undefined;
}
