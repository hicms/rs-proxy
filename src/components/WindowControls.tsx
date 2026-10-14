import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { HStack } from '@astryxdesign/core/Layout';
import { IconButton } from '@astryxdesign/core/IconButton';
import { Icon } from '@astryxdesign/core/Icon';
import { Copy, Minus, Square, X } from 'lucide-react';
import type { useWindowChrome } from '../useWindowChrome';

export function WindowControls({ chrome }: { chrome: ReturnType<typeof useWindowChrome> }) {
  const [modal, setModal] = useState<HTMLDialogElement | null>(null);
  const controls = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!chrome.custom) return;
    const sync = () => {
      const focused = document.activeElement?.closest<HTMLDialogElement>('dialog:modal');
      const dialogs = document.querySelectorAll<HTMLDialogElement>('dialog:modal');
      setModal(focused ?? dialogs.item(dialogs.length - 1));
    };
    const observer = new MutationObserver(sync);
    observer.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['open'] });
    document.addEventListener('focusin', sync);
    sync();
    return () => { observer.disconnect(); document.removeEventListener('focusin', sync); };
  }, [chrome.custom]);

  useLayoutEffect(() => {
    if (modal?.matches(':modal') && controls.current) {
      controls.current.popover = 'manual';
      controls.current.showPopover();
    }
  }, [modal]);

  if (!chrome.custom) return null;
  const maximizeLabel = chrome.maximized ? '还原窗口' : '最大化窗口';
  const buttons = <HStack ref={controls} gap={0} vAlign="center" data-tauri-drag-region="false"
    className={modal ? 'window-controls window-controls-modal' : 'window-controls'}>
    <IconButton label="最小化窗口" tooltip="最小化窗口" icon={<Icon icon={Minus} size="sm" />} variant="ghost" size="lg" onClick={() => void chrome.run('minimize')} />
    <IconButton label={maximizeLabel} tooltip={maximizeLabel} icon={<Icon icon={chrome.maximized ? Copy : Square} size="sm" />} variant="ghost" size="lg" onClick={() => void chrome.run('maximize')} />
    <IconButton label="关闭窗口" tooltip="关闭窗口" icon={<Icon icon={X} size="sm" />} variant="ghost" size="lg" onClick={() => void chrome.run('close')} />
  </HStack>;

  // Modal dialogs make the rest of the WebView inert; caption controls need the top layer.
  return modal ? <><HStack className="window-controls-placeholder" aria-hidden="true" />{createPortal(buttons, modal)}</> : buttons;
}
