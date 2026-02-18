#!/usr/bin/env node

/**
 * Auto-detect OS and run appropriate build script
 * This is a convenience wrapper that calls the platform-specific build script
 */

import { spawn } from 'child_process';
import os from 'os';
import path from 'path';
import { fileURLToPath } from 'url';

// Derive __dirname for ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const platform = os.platform();
const args = process.argv.slice(2);

function runBuild() {
  let command;
  let scriptArgs = args;

  switch (platform) {
    case 'win32':
      // Windows - use PowerShell script
      command = 'powershell';
      scriptArgs = [
        '-ExecutionPolicy', 'Bypass',
        '-File', path.join(__dirname, 'build.ps1'),
        ...convertArgsForPowerShell(args)
      ];
      console.log('🪟  Detected Windows - using PowerShell build script');
      break;

    case 'darwin':
    case 'linux':
      // macOS/Linux - use Bash script
      command = path.join(__dirname, 'build.sh');
      console.log(`🐧  Detected ${platform === 'darwin' ? 'macOS' : 'Linux'} - using Bash build script`);
      break;

    default:
      // Fallback to Node.js script
      console.log(`⚠️  Unknown platform: ${platform} - using Node.js build script`);
      command = 'node';
      scriptArgs = [path.join(__dirname, 'build.js'), ...args];
      break;
  }

  console.log(`\n🚀 Starting build with: ${command} ${scriptArgs.join(' ')}\n`);

  const proc = spawn(command, scriptArgs, {
    stdio: 'inherit',
    shell: platform === 'win32'
  });

  proc.on('close', (code) => {
    process.exit(code);
  });

  proc.on('error', (error) => {
    console.error(`❌ Failed to start build: ${error.message}`);
    process.exit(1);
  });
}

/**
 * Convert Node-style args to PowerShell-style args
 * --mode dev => -Mode dev
 * --clean => -Clean
 */
function convertArgsForPowerShell(args) {
  const converted = [];
  
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    
    if (arg.startsWith('--')) {
      // Convert --option to -Option (capitalize first letter)
      const optionName = arg.slice(2);
      const psOption = `-${optionName.charAt(0).toUpperCase()}${optionName.slice(1)}`;
      converted.push(psOption);
    } else {
      converted.push(arg);
    }
  }
  
  return converted;
}

// Show help if requested
if (args.includes('--help') || args.includes('-h')) {
  console.log(`
╔══════════════════════════════════════════════════════════╗
║                                                          ║
║        Vault Desktop Auto Build (OS Detection)          ║
║                                                          ║
╚══════════════════════════════════════════════════════════╝

This script automatically detects your OS and runs the appropriate build script:
  - Windows  → build.ps1 (PowerShell)
  - macOS    → build.sh (Bash)
  - Linux    → build.sh (Bash)
  - Other    → build.js (Node.js)

Usage:
  node build-auto.js [options]
  
  All options are passed through to the platform-specific script.

Examples:
  node build-auto.js
  node build-auto.js --mode dev --no-bundle
  node build-auto.js --clean --checksums

For detailed help on available options:
  - Windows:  Get-Help .\\build.ps1 -Full
  - Unix:     ./build.sh --help
  - Node.js:  node build.js --help
`);
  process.exit(0);
}

// Run the build
runBuild();

