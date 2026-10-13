import { HStack, VStack } from '@astryxdesign/core/Layout';
import { Text } from '@astryxdesign/core/Text';
import { Icon } from '@astryxdesign/core/Icon';
import { ArrowUp, ArrowDown } from 'lucide-react';
import type { TrafficStats } from '../bindings/TrafficStats';
import { bytes } from '../model';

export function TrafficRate({ stats }: { stats?: TrafficStats }) {
  return <VStack gap={0.5}>
    <HStack gap={1} vAlign="center"><Icon icon={ArrowUp} size="sm" color="secondary" label="上传速度" /><Text type="supporting" hasTabularNumbers>{bytes(stats?.upload_bytes_per_second ?? 0)}/s</Text></HStack>
    <HStack gap={1} vAlign="center"><Icon icon={ArrowDown} size="sm" color="secondary" label="下载速度" /><Text type="supporting" hasTabularNumbers>{bytes(stats?.download_bytes_per_second ?? 0)}/s</Text></HStack>
  </VStack>;
}

export function TrafficSummary({ stats, label }: { stats?: TrafficStats; label: string }) {
  return <VStack gap={2}>
    <Text type="supporting" color="secondary">{label}</Text>
    <HStack gap={6} wrap="wrap">
      <VStack gap={0.5}><HStack gap={1} vAlign="center"><Icon icon={ArrowUp} size="sm" label="上传速度" /><Text weight="semibold" hasTabularNumbers>{bytes(stats?.upload_bytes_per_second ?? 0)}/s</Text></HStack><Text type="supporting" color="secondary" hasTabularNumbers>已上传 {bytes(stats?.uploaded_bytes ?? 0)}</Text></VStack>
      <VStack gap={0.5}><HStack gap={1} vAlign="center"><Icon icon={ArrowDown} size="sm" label="下载速度" /><Text weight="semibold" hasTabularNumbers>{bytes(stats?.download_bytes_per_second ?? 0)}/s</Text></HStack><Text type="supporting" color="secondary" hasTabularNumbers>已下载 {bytes(stats?.downloaded_bytes ?? 0)}</Text></VStack>
    </HStack>
  </VStack>;
}
