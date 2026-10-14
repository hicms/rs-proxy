import type { ReactNode } from 'react';
import { Dialog, DialogHeader } from '@astryxdesign/core/Dialog';
import { Layout, LayoutContent, LayoutFooter, HStack, VStack } from '@astryxdesign/core/Layout';
import { Button } from '@astryxdesign/core/Button';
import { Banner } from '@astryxdesign/core/Banner';
import { Selector } from '@astryxdesign/core/Selector';
import { Icon } from '@astryxdesign/core/Icon';
import { Check, Trash2 } from 'lucide-react';
import type { ProxyGroup } from '../bindings/ProxyGroup';
import { useAction } from '../model';
import { PrimaryButton } from './PageHeading';
import { usePageVisible } from './RetainedView';

export function Editor({ title, children, onClose, onSave, saveLabel = '保存', destructive = false }: {
  title: string; children: ReactNode; onClose: () => void; onSave: () => Promise<unknown>; saveLabel?: string; destructive?: boolean;
}) {
  const action = useAction();
  const visible = usePageVisible();
  const close = () => { if (!action.busy) onClose(); };
  if (!visible) return null;
  return <Dialog isOpen onOpenChange={close} purpose="form" width="64ch" maxHeight="85dvh">
    <Layout padding={6} height="fill" header={<DialogHeader title={title} onOpenChange={close} />}
      content={<LayoutContent><VStack gap={4}>{action.error && <Banner status="error" title="操作未完成" description={action.error} />}{children}</VStack></LayoutContent>}
      footer={<LayoutFooter hasDivider><HStack gap={2} hAlign="end">
        <Button label="取消" onClick={close} isDisabled={action.busy} />
        <PrimaryButton label={saveLabel} icon={<Icon icon={destructive ? Trash2 : Check} />} variant={destructive ? 'destructive' : 'primary'} isLoading={action.busy} onClick={() => void action.run(onSave).then(saved => { if (saved) onClose(); })} />
      </HStack></LayoutFooter>} />
  </Dialog>;
}
export function GroupSelector({ groups, value, onChange, label = '代理组', isDisabled = false }: { groups: ProxyGroup[]; value: number; onChange: (value: number) => void; label?: string; isDisabled?: boolean }) {
  return <Selector label={label} value={String(value)} isDisabled={isDisabled} width="100%" options={groups.map(group => ({ value: String(group.id), label: group.name }))} onChange={value => onChange(Number(value))} />;
}
