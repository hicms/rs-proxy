import { realpathSync } from 'node:fs';

// Vite and esbuild must agree on one root when the project is opened through a junction.
process.chdir(realpathSync(process.cwd()));
const vite = await import('vite');
const command = process.argv[2];
if (command === 'build') {
  await vite.build();
} else if (command === 'dev') {
  const server = await vite.createServer();
  await server.listen();
  server.printUrls();
} else if (command === 'preview') {
  const server = await vite.preview({ preview: { host: '127.0.0.1' } });
  server.printUrls();
} else {
  throw new Error(`Unknown frontend command: ${command}`);
}
