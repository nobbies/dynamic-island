import { spawnSync } from 'node:child_process';

const args = process.argv.slice(2);
if (args[0] === 'dev') args.push('--config', 'src-tauri/tauri.dev.conf.json');

const result = spawnSync(process.execPath, ['node_modules/@tauri-apps/cli/tauri.js', ...args], { stdio: 'inherit' });
if (result.error) throw result.error;
if (result.status === null) throw new Error(`Tauri CLI terminated by ${result.signal}`);
if (result.status !== 0) process.exitCode = result.status;
