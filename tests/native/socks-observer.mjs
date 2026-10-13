import { createServer, createConnection } from 'node:net';
import { once } from 'node:events';
import { createSocket } from 'node:dgram';

async function read(socket, length) {
  const chunks = []; let count = 0;
  while (count < length) {
    const chunk = socket.read(length - count);
    if (chunk) { chunks.push(chunk); count += chunk.length; }
    else { if (socket.destroyed || socket.readableEnded) throw new Error('SOCKS EOF'); await once(socket, 'readable'); }
  }
  return Buffer.concat(chunks);
}
async function address(socket, type) {
  if (type === 1) return await read(socket, 6);
  if (type === 4) return await read(socket, 18);
  if (type === 3) { const length = await read(socket, 1); return Buffer.concat([length, await read(socket, length[0] + 2)]); }
  throw new Error(`Unsupported address type ${type}`);
}

// A transparent test observer: every SOCKS reply and application byte comes from the real upstream.
export async function observeSocks(host = '127.0.0.1', port = 7891) {
  const records = []; const sockets = new Set(); const errors = [];
  const track = socket => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); socket.on('error', error => errors.push(error.message)); return socket; };
  const server = createServer({ allowHalfOpen: true }, client => {
    track(client);
    void (async () => {
      const upstream = track(createConnection({ host, port, allowHalfOpen: true }));
      client.once('close', () => upstream.destroy()); upstream.once('close', () => client.destroy());
      await once(upstream, 'connect');
      const greeting = await read(client, 2); const methods = await read(client, greeting[1]);
      upstream.write(Buffer.concat([greeting, methods])); client.write(await read(upstream, 2));
      // A connection test ends after greeting without issuing CONNECT.
      const header = await read(client, 4); const destination = await address(client, header[3]);
      const record = { command: header[1], type: header[3], target: header[3] === 1 ? `${[...destination.subarray(0, 4)].join('.')}:${destination.readUInt16BE(4)}` : 'other', uploaded: 0, downloaded: 0, datagrams: [] };
      records.push(record);
      let relay, udp;
      if (header[1] === 3) {
        relay = createSocket('udp4'); udp = createSocket('udp4');
        relay.on('error', error => errors.push(error.message)); udp.on('error', error => errors.push(error.message));
        relay.bind(0, '127.0.0.1'); udp.bind(0, '127.0.0.1'); await Promise.all([once(relay, 'listening'), once(udp, 'listening')]);
        client.once('close', () => { relay.close(); udp.close(); });
        const source = Buffer.from([0, 0, 0, 0, 0, 0]); source.writeUInt16BE(udp.address().port, 4);
        upstream.write(Buffer.concat([Buffer.from([5, 3, 0, 1]), source]));
      } else upstream.write(Buffer.concat([header, destination]));
      const response = await read(upstream, 4); const bound = await address(upstream, response[3]);
      record.reply = response[1];
      if (relay && response[1] === 0) {
        if (response[3] !== 1) throw new Error('Observer expects IPv4 UDP relay');
        const upstreamHost = bound.subarray(0, 4).every(value => value === 0) ? host : [...bound.subarray(0, 4)].join('.');
        const upstreamPort = bound.readUInt16BE(4); let peer;
        record.relay = `${upstreamHost}:${upstreamPort}`;
        relay.on('message', (bytes, source) => {
          peer = source;
          if (bytes.length >= 12 && bytes[3] === 1) record.datagrams.push({ direction: 'request', target: `${[...bytes.subarray(4, 8)].join('.')}:${bytes.readUInt16BE(8)}`, id: bytes.readUInt16BE(10), bytes: bytes.length - 10 });
          udp.send(bytes, upstreamPort, upstreamHost);
        });
        udp.on('message', (bytes, source) => {
          if (bytes.length >= 12 && bytes[3] === 1) record.datagrams.push({ direction: 'response', source: `${source.address}:${source.port}`, target: `${[...bytes.subarray(4, 8)].join('.')}:${bytes.readUInt16BE(8)}`, id: bytes.readUInt16BE(10), bytes: bytes.length - 10 });
          if (peer) relay.send(bytes, peer.port, peer.address);
        });
        const local = Buffer.from([5, 0, 0, 1, 127, 0, 0, 1, 0, 0]); local.writeUInt16BE(relay.address().port, 8); client.write(local);
      } else client.write(Buffer.concat([response, bound]));
      client.on('data', bytes => record.uploaded += bytes.length); upstream.on('data', bytes => record.downloaded += bytes.length);
      client.pipe(upstream); upstream.pipe(client);
    })().catch(error => { if (error.message !== 'SOCKS EOF') errors.push(error.message); client.destroy(); });
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  return { port: server.address().port, records, errors, close: async () => { for (const socket of sockets) socket.destroy(); server.close(); await once(server, 'close'); } };
}
