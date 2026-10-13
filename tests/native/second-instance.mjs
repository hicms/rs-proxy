import { spawn } from 'node:child_process';

export async function focusExistingInstance() {
  if (!process.env.RS_PROXY_E2E_EXE) throw new Error('QA executable is required');
  const second = spawn(process.env.RS_PROXY_E2E_EXE, [], { windowsHide: true });
  let timer;
  try {
    await Promise.race([
      new Promise((resolve, reject) => {
        second.once('error', reject);
        second.once('exit', code => code === 0 ? resolve() : reject(new Error(`second instance exit ${code}`)));
      }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('second instance did not exit')), 10000); }),
    ]);
  } finally {
    clearTimeout(timer);
    if (second.pid && second.exitCode === null && second.signalCode === null) {
      const stopped = new Promise(resolve => second.once('exit', resolve));
      second.kill();
      await Promise.race([stopped, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('owned second instance did not stop')), 5000); })]).finally(() => clearTimeout(timer));
    }
  }
}
