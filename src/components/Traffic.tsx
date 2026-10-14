import { HStack, VStack } from '@astryxdesign/core/Layout';
import { Stack } from '@astryxdesign/core/Stack';
import { Text } from '@astryxdesign/core/Text';
import { Icon } from '@astryxdesign/core/Icon';
import { Tooltip } from '@astryxdesign/core/Tooltip';
import { ArrowUp, ArrowDown } from 'lucide-react';
import type { TrafficStats } from '../bindings/TrafficStats';
import { bytes } from '../model';

export function TrafficRate({ stats }: { stats?: TrafficStats }) {
  return <VStack gap={0.5}>
    <HStack gap={1} vAlign="center"><Icon icon={ArrowUp} size="sm" color="secondary" label="上传速度" /><Text type="supporting" hasTabularNumbers>{bytes(stats?.upload_bytes_per_second ?? 0)}/s</Text></HStack>
    <HStack gap={1} vAlign="center"><Icon icon={ArrowDown} size="sm" color="secondary" label="下载速度" /><Text type="supporting" hasTabularNumbers>{bytes(stats?.download_bytes_per_second ?? 0)}/s</Text></HStack>
  </VStack>;
}

export function TrafficSummary({ stats, label, compact = false }: { stats?: TrafficStats; label: string; compact?: boolean }) {
  const values = <HStack gap={compact ? 3 : 6} wrap="wrap" role="group" aria-label={label}>
      <Stack direction={compact ? 'horizontal' : 'vertical'} gap={compact ? 2 : 0.5}>
        <HStack gap={1} vAlign="center"><Icon icon={ArrowUp} size="sm" label="上传速度" /><Text type={compact ? 'supporting' : 'body'} weight="semibold" hasTabularNumbers textWrap="nowrap">{bytes(stats?.upload_bytes_per_second ?? 0)}/s</Text></HStack><Text type="supporting" color="secondary" hasTabularNumbers textWrap="nowrap">已上传 {bytes(stats?.uploaded_bytes ?? 0)}</Text>
      </Stack>
      <Stack direction={compact ? 'horizontal' : 'vertical'} gap={compact ? 2 : 0.5}>
        <HStack gap={1} vAlign="center"><Icon icon={ArrowDown} size="sm" label="下载速度" /><Text type={compact ? 'supporting' : 'body'} weight="semibold" hasTabularNumbers textWrap="nowrap">{bytes(stats?.download_bytes_per_second ?? 0)}/s</Text></HStack><Text type="supporting" color="secondary" hasTabularNumbers textWrap="nowrap">已下载 {bytes(stats?.downloaded_bytes ?? 0)}</Text>
      </Stack>
    </HStack>;
  return compact ? <Tooltip content={label} hasHoverIndication={false}>{values}</Tooltip> : <VStack gap={2}>
    <Text type="supporting" color="secondary">{label}</Text>
    {values}
  </VStack>;
}
