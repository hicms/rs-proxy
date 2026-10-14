import { Table, proportional, type TableColumn } from '@astryxdesign/core/Table';
import { Text } from '@astryxdesign/core/Text';
import { EmptyState } from '@astryxdesign/core/EmptyState';
import { memo } from 'react';
import type { Connection } from '../bindings/Connection';
const columns: TableColumn<Connection>[] = [
  { key: 'pid', header: 'PID' },
  { key: 'protocol', header: '协议', renderCell: item => <Text>{item.protocol.toUpperCase()}</Text> },
  { key: 'local_address', header: '本地地址', width: proportional(2), renderCell: item => <Text type="code">{item.local_address}:{item.local_port}</Text> },
  { key: 'remote_address', header: '目标地址', width: proportional(2), renderCell: item => <Text type="code">{item.remote_address}:{item.remote_port}</Text> },
  { key: 'state', header: '状态' },
];
export const Connections = memo(function Connections({ data }: { data: Connection[] }) {
  if (!data.length) return <EmptyState title="暂无连接" description="建立 IPv4 TCP 或 UDP 连接后会显示在这里。" />;
  return <Table data={data} columns={columns} idKey={item => `${item.pid}-${item.protocol}-${item.local_address}:${item.local_port}-${item.remote_address}:${item.remote_port}`} density="compact" hasHover />;
});
