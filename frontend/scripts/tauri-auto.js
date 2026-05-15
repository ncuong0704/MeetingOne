#!/usr/bin/env node
/**
 * Auto-detect GPU and run Tauri with appropriate features
 */

const { execSync } = require('child_process');
const path = require('path');
const fs = require('fs');
const os = require('os');

// Get the command (dev or build) and any extra args forwarded by tauri-action
const command = process.argv[2];
if (!command || !['dev', 'build'].includes(command)) {
  console.error('Usage: node tauri-auto.js [dev|build] [extra args...]');
  process.exit(1);
}
const extraArgs = process.argv.slice(3).join(' '); // e.g. "--target aarch64-apple-darwin"

// Detect GPU feature
let feature = '';

// Check for environment variable override first
if (process.env.TAURI_GPU_FEATURE) {
  feature = process.env.TAURI_GPU_FEATURE;
  console.log(`🔧 Using forced GPU feature from environment: ${feature}`);
} else {
  try {
    const result = execSync('node scripts/auto-detect-gpu.js', {
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'inherit']
    });
    feature = result.trim();
  } catch (err) {
    // If detection fails, continue with no features
  }
}

console.log(''); // Empty line for spacing

// Platform-specific environment variables
const platform = os.platform();
const env = { ...process.env };

// Ensure node_modules/.bin is in PATH so `tauri` binary is found
// (needed when called directly via tauriScript instead of pnpm run)
const localBin = path.join(__dirname, '..', 'node_modules', '.bin');
const pathSep = platform === 'win32' ? ';' : ':';
env.PATH = `${localBin}${pathSep}${env.PATH || process.env.PATH}`;

if (platform === 'linux' && feature === 'cuda') {
  console.log('🐧 Linux/CUDA detected: Setting CMAKE flags for NVIDIA GPU');
  env.CMAKE_CUDA_ARCHITECTURES = '75';
  env.CMAKE_CUDA_STANDARD = '17';
  env.CMAKE_POSITION_INDEPENDENT_CODE = 'ON';
}

// Build the tauri command — forward extra args (e.g. --target) then append features
let tauriCmd = `tauri ${command}`;
if (extraArgs) tauriCmd += ` ${extraArgs}`;
if (feature && feature !== 'none') {
  tauriCmd += ` -- --features ${feature}`;
  console.log(`🚀 Running: tauri ${command}${extraArgs ? ' ' + extraArgs : ''} with features: ${feature}`);
} else {
  console.log(`🚀 Running: tauri ${command}${extraArgs ? ' ' + extraArgs : ''} (CPU-only mode)`);
}
console.log('');

// Execute the command
try {
  execSync(tauriCmd, { stdio: 'inherit', env });
} catch (err) {
  process.exit(err.status || 1);
}
