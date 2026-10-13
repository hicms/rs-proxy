import { createConnection } from 'node:net';
import { createSocket } from 'node:dgram';
import { createInterface } from 'node:readline';
import assert from 'node:assert/strict';

const [mode, host, name = 'one.test'] = process.argv.slice(2);
const input = createInterface({ input: process.stdin });
console.log('READY');
for await (const line of input) { if (line === 'GO') break; }
input.close();
const query = Buffer.alloc(12); query.writeUInt16BE(0x1234); query[2] = 1; query[5] = 1;
const labels = name.split('.').flatMap(label => [Buffer.from([label.length]), Buffer.from(label)]);
const payload = Buffer.concat([query, ...labels, Buffer.from([0, 0, 1, 0, 1])]);
const timer = setTimeout(() => { console.error('DNS fixture timed out'); process.exit(1); }, 6000);
try {
  const tcp = () => new Promise((resolve, reject) => {
      const socket = createConnection({ host, port: 53 });
      let buffer = Buffer.alloc(0);
      socket.on('error', reject);
      socket.on('connect', () => { const length = Buffer.alloc(2); length.writeUInt16BE(payload.length); socket.write(Buffer.concat([length, payload])); });
      socket.on('data', bytes => {
        buffer = Buffer.concat([buffer, bytes]);
        if (buffer.length >= 2 && buffer.length >= buffer.readUInt16BE(0) + 2) {
          socket.destroy(); resolve(buffer.subarray(2, buffer.readUInt16BE(0) + 2));
        }
      });
    });
  let response;
  let truncatedBytes = 0;
  if (mode === 'tcp') {
    response = await tcp();
  } else if (mode === 'rebind' || mode === 'silent') {
    let socket = createSocket('udp4');
    await new Promise(resolve => socket.bind(0, resolve));
    const port = socket.address().port;
    await new Promise((resolve, reject) => socket.send(payload, 53, host, error => error ? reject(error) : resolve()));
    if (mode === 'rebind') {
      await new Promise(resolve => setTimeout(resolve, 100));
      await new Promise(resolve => socket.close(resolve));
      socket = createSocket('udp4');
      await new Promise(resolve => socket.bind(port, resolve));
    }
    let received = false;
    socket.on('message', () => { received = true; });
    await new Promise(resolve => setTimeout(resolve, 1500));
    socket.close();
    assert.equal(received, false, 'a stale or invalid DNS response was delivered');
    console.log(JSON.stringify({ bytes: 0, queryBytes: payload.length }));
  } else {
    response = await new Promise((resolve, reject) => {
      const socket = createSocket('udp4');
      socket.on('error', reject);
      socket.on('message', (bytes, peer) => {
        try { assert.equal(peer.address, host); assert.equal(peer.port, 53); socket.close(); resolve(bytes); }
        catch (error) { reject(error); }
      });
      socket.send(payload, 53, host);
    });
    if (mode === 'retry') {
      assert.ok(response[2] & 2, 'oversized UDP response must set TC');
      assert.ok(response.length <= 512);
      assert.deepEqual(response.subarray(12), payload.subarray(12));
      truncatedBytes = response.length;
      response = await tcp();
      assert.ok(response.length > 512, 'TCP retry must deliver the full answer');
    }
  }
  if (response) {
  assert.equal(response.readUInt16BE(0), 0x1234);
  assert.ok(response[2] & 128);
  assert.deepEqual(response.subarray(12, payload.length), payload.subarray(12));
  console.log(JSON.stringify({ bytes: response.length, queryBytes: payload.length, truncatedBytes, response: response.toString('base64') }));
  }
} finally { clearTimeout(timer); }
