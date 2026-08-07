#!/usr/bin/env node
/**
 * Thin wrapper around the `tauri` CLI that ensures `node_modules/.bin` is on PATH.
 * Needed because `tauri-apps/tauri-action` (used in CI) invokes this script directly
 * rather than through `pnpm run`, so the locally-installed `tauri` binary wouldn't
 * otherwise be found.
 */

const { execSync } = require('child_process');
const path = require('path');
const os = require('os');

// Get the command (dev or build) and any extra args forwarded by tauri-action
const command = process.argv[2];
if (!command || !['dev', 'build'].includes(command)) {
  console.error('Usage: node tauri-auto.js [dev|build] [extra args...]');
  process.exit(1);
}
const extraArgs = process.argv.slice(3).join(' '); // e.g. "--target aarch64-apple-darwin"

const platform = os.platform();
const env = { ...process.env };

// Ensure node_modules/.bin is in PATH so `tauri` binary is found
// (needed when called directly via tauriScript instead of pnpm run)
const localBin = path.join(__dirname, '..', 'node_modules', '.bin');
const pathSep = platform === 'win32' ? ';' : ':';
env.PATH = `${localBin}${pathSep}${env.PATH || process.env.PATH}`;

let tauriCmd = `tauri ${command}`;
if (extraArgs) tauriCmd += ` ${extraArgs}`;
console.log(`🚀 Running: ${tauriCmd}`);
console.log('');

try {
  execSync(tauriCmd, { stdio: 'inherit', env });
} catch (err) {
  process.exit(err.status || 1);
}
