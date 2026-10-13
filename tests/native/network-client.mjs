import { createInterface } from 'node:readline';
import { get } from 'node:https';
import { createSocket } from 'node:dgram';
import { spawn } from 'node:child_process';
import { createConnection } from 'node:net';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const output = value => process.stdout.write(`${JSON.stringify(value)}\n`);
let child;
let childLines;
let held;
let heldDns;
let heldDnsError;
function dnsQuery(id) {
  const header = Buffer.alloc(12); header.writeUInt16BE(id); header[2] = 1; header[5] = 1;
  return Buffer.concat([header, Buffer.from('\x07example\x03com\0\0\x01\0\x01', 'binary')]);
}
async function request(command) {
  if (command.kind === 'dns-open') {
    heldDns?.destroy(); heldDnsError = undefined;
    heldDns = createConnection({ host: '1.1.1.1', port: 53 });
    heldDns.on('error', error => { heldDnsError = error.code; });
    await once(heldDns, 'connect');
    return { connected: true, pid: process.pid };
  }
  if (command.kind === 'dns-state') return { closed: heldDns.destroyed, error: heldDnsError, pid: process.pid };
  if (command.kind === 'dns-query') return await new Promise((resolve, reject) => {
    let buffer = Buffer.alloc(0);
    const done = (error, value) => {
      clearTimeout(timer); heldDns.off('data', received); heldDns.off('error', failed); heldDns.off('end', ended);
      if (error) reject(error); else resolve(value);
    };
    const failed = error => done(error);
    const ended = () => done(new Error('TCP DNS EOF'));
    const received = bytes => {
      buffer = Buffer.concat([buffer, bytes]);
      if (buffer.length < 2 || buffer.length < buffer.readUInt16BE(0) + 2) return;
      try {
        const reply = buffer.subarray(2); assert.ok(reply.length >= 12);
        assert.equal(reply.readUInt16BE(0), command.id); assert.ok(reply[2] & 128);
        assert.equal(reply[3] & 15, 0); assert.ok(reply.readUInt16BE(6) > 0);
        done(null, { bytes: reply.length, pid: process.pid });
      } catch (error) { done(error); }
    };
    const timer = setTimeout(() => done(new Error('TCP DNS timeout')), 10000);
    heldDns.on('data', received); heldDns.once('error', failed); heldDns.once('end', ended);
    const payload = dnsQuery(command.id); const length = Buffer.alloc(2); length.writeUInt16BE(payload.length);
    heldDns.write(Buffer.concat([length, payload]));
  });
  if (command.kind === 'hold') { held = createConnection({ host: '1.1.1.1', port: 443 }); await once(held, 'connect'); return { connected: true }; }
  if (command.kind === 'release') { held?.destroy(); return { closed: true }; }
  if (command.kind === 'https') return await new Promise((resolve, reject) => {
    const req = get(`https://${command.host ?? '1.1.1.1'}/cdn-cgi/trace?rsproxy=${command.nonce}`, { agent: false, timeout: 10000 }, response => {
      let body = ''; response.setEncoding('utf8'); response.on('data', chunk => body += chunk);
      response.on('end', () => { try { assert.equal(response.statusCode, 200); assert.match(body, /ip=/); resolve({ status: response.statusCode, bytes: Buffer.byteLength(body) }); } catch (error) { reject(error); } });
    });
    req.on('timeout', () => req.destroy(new Error('HTTPS timeout'))); req.on('error', reject);
  });
  if (command.kind === 'udp') {
    const socket = createSocket('udp4');
    try {
      const results = [];
      for (const [index, host] of (command.hosts ?? ['1.1.1.1', '8.8.8.8']).entries()) {
        const id = (command.id + index) & 65535;
        const result = await new Promise((resolve, reject) => {
          const timer = setTimeout(() => { socket.removeAllListeners('message'); reject(new Error(`UDP timeout: ${host}`)); }, 10000);
          socket.once('error', error => { clearTimeout(timer); reject(error); });
          socket.once('message', (bytes, peer) => {
            clearTimeout(timer); socket.removeAllListeners('error');
            try { assert.equal(peer.address, host); assert.equal(peer.port, 53); assert.equal(bytes.readUInt16BE(0), id); assert.ok(bytes[2] & 128); assert.equal(bytes[3] & 15, 0); assert.ok(bytes.readUInt16BE(6) > 0); resolve({ host, id, bytes: bytes.length }); } catch (error) { reject(error); }
          });
          socket.send(dnsQuery(id), 53, host);
        });
        results.push(result);
      }
      return results;
    } finally { socket.close(); }
  }
  if (command.kind === 'spawn') {
    assert.ok(!child, 'child already running');
    child = spawn(process.execPath, [fileURLToPath(import.meta.url), '--rs-proxy-e2e-descendant'], { stdio: ['pipe', 'pipe', 'inherit'], windowsHide: true });
    childLines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    return JSON.parse((await childLines.next()).value);
  }
  if (command.kind === 'child') { child.stdin.write(`${JSON.stringify(command.request)}\n`); return JSON.parse((await childLines.next()).value); }
  throw new Error(`Unknown operation ${command.kind}`);
}
output({ ready: true, pid: process.pid });
for await (const line of createInterface({ input: process.stdin })) {
  try { output({ ok: true, result: await request(JSON.parse(line)) }); }
  catch (error) { output({ ok: false, error: error.stack }); }
}
child?.kill();
held?.destroy();
heldDns?.destroy();
