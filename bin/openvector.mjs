#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { nativePath } from '../npm/native.mjs';

try {
  const executable = nativePath(process.platform === 'win32' ? 'openvector.exe' : 'openvector', true);
  const args = process.argv.slice(2);
  if (process.platform !== 'win32') {
    if (typeof process.execve !== 'function') {
      throw new Error('OpenVector requires Node.js 22.19 or later with process.execve support.');
    }
    // Keep the PID and standard descriptors, so signals reach Rust exactly once.
    process.execve(executable, [executable, ...args], process.env);
  } else {
    // Both processes inherit the console's Ctrl-C/Ctrl-Break events. Rust owns
    // cancellation; child.kill('SIGINT') on Windows would forcibly terminate it.
    const ignoreConsoleSignal = () => {};
    process.on('SIGINT', ignoreConsoleSignal);
    process.on('SIGBREAK', ignoreConsoleSignal);
    const child = spawn(executable, args, { stdio: 'inherit', windowsHide: false });
    child.on('error', (error) => {
      console.error(`OpenVector could not start ${executable}: ${error.message}`);
      process.exitCode = 1;
    });
    child.on('close', (code) => {
      process.removeListener('SIGINT', ignoreConsoleSignal);
      process.removeListener('SIGBREAK', ignoreConsoleSignal);
      process.exitCode = code ?? 1;
    });
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
