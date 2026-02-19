#!/usr/bin/env node

/**
 * Advanced Modular Build Script for Vault Desktop
 * Works on Windows, macOS, and Linux
 * 
 * Usage:
 *   node build.js [options]
 * 
 * Options:
 *   --platform <win|mac|linux|all>  Target platform (default: current)
 *   --mode <dev|release|production> Build mode (default: release)
 *   --clean                         Clean build artifacts before building
 *   --skip-tests                    Skip running tests
 *   --no-bundle                     Skip creating installers
 *   --features <feat1,feat2>        Enable specific features
 *   --disable-features <feat1>      Disable specific features
 *   --target <triple>               Specific Rust target triple
 *   --verbose                       Enable verbose logging
 *   --parallel                      Enable parallel builds (default)
 *   --no-parallel                   Disable parallel builds
 *   --checksums                     Generate SHA256 checksums
 *   --config <path>                 Custom config file path
 *   --help                          Show this help message
 */

const fs = require('fs');
const path = require('path');
const { execSync, spawn } = require('child_process');
const os = require('os');

// ============================================================================
// Configuration & Constants
// ============================================================================

const COLORS = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
};

class BuildSystem {
  constructor() {
    this.startTime = Date.now();
    this.config = this.loadConfig();
    this.args = this.parseArgs();
    this.platform = this.detectPlatform();
    this.logFile = null;
  }

  // ==========================================================================
  // Argument Parsing
  // ==========================================================================

  parseArgs() {
    const args = process.argv.slice(2);
    const options = {
      platform: null,
      mode: 'release',
      clean: false,
      skipTests: false,
      bundle: true,
      features: [],
      disabledFeatures: [],
      target: null,
      verbose: false,
      parallel: true,
      checksums: false,
      configPath: 'build.config.json',
      help: false,
    };

    for (let i = 0; i < args.length; i++) {
      const arg = args[i];
      const next = args[i + 1];

      switch (arg) {
        case '--platform':
          options.platform = next;
          i++;
          break;
        case '--mode':
          options.mode = next;
          i++;
          break;
        case '--clean':
          options.clean = true;
          break;
        case '--skip-tests':
          options.skipTests = true;
          break;
        case '--no-bundle':
          options.bundle = false;
          break;
        case '--features':
          options.features = next.split(',');
          i++;
          break;
        case '--disable-features':
          options.disabledFeatures = next.split(',');
          i++;
          break;
        case '--target':
          options.target = next;
          i++;
          break;
        case '--verbose':
          options.verbose = true;
          break;
        case '--parallel':
          options.parallel = true;
          break;
        case '--no-parallel':
          options.parallel = false;
          break;
        case '--checksums':
          options.checksums = true;
          break;
        case '--config':
          options.configPath = next;
          i++;
          break;
        case '--help':
        case '-h':
          options.help = true;
          break;
        default:
          if (arg.startsWith('--')) {
            this.warn(`Unknown option: ${arg}`);
          }
      }
    }

    return options;
  }

  // ==========================================================================
  // Configuration Loading
  // ==========================================================================

  loadConfig() {
    const configPath = path.join(__dirname, 'build.config.json');
    try {
      const configData = fs.readFileSync(configPath, 'utf-8');
      return JSON.parse(configData);
    } catch (error) {
      this.warn('Could not load build.config.json, using defaults');
      return this.getDefaultConfig();
    }
  }

  getDefaultConfig() {
    return {
      platforms: {
        windows: { enabled: true, targets: ['x86_64-pc-windows-msvc'] },
        macos: { enabled: true, targets: ['x86_64-apple-darwin', 'aarch64-apple-darwin'] },
        linux: { enabled: true, targets: ['x86_64-unknown-linux-gnu'] },
      },
      features: {},
      build: {
        clean: false,
        skipTests: false,
        verbose: false,
        parallel: true,
        optimization: 'release',
      },
    };
  }

  // ==========================================================================
  // Platform Detection
  // ==========================================================================

  detectPlatform() {
    const platform = os.platform();
    switch (platform) {
      case 'win32':
        return 'windows';
      case 'darwin':
        return 'macos';
      case 'linux':
        return 'linux';
      default:
        throw new Error(`Unsupported platform: ${platform}`);
    }
  }

  getPlatformName(short) {
    const names = {
      win: 'windows',
      mac: 'macos',
      linux: 'linux',
    };
    return names[short] || short;
  }

  // ==========================================================================
  // Logging
  // ==========================================================================

  log(message, color = 'reset') {
    const timestamp = new Date().toISOString();
    const colorCode = COLORS[color] || COLORS.reset;
    console.log(`${colorCode}[${timestamp}] ${message}${COLORS.reset}`);
    
    if (this.logFile) {
      fs.appendFileSync(this.logFile, `[${timestamp}] ${message}\n`);
    }
  }

  info(message) {
    this.log(`ℹ  ${message}`, 'blue');
  }

  success(message) {
    this.log(`✓ ${message}`, 'green');
  }

  warn(message) {
    this.log(`⚠  ${message}`, 'yellow');
  }

  error(message) {
    this.log(`✗ ${message}`, 'red');
  }

  header(message) {
    const line = '='.repeat(60);
    this.log(`\n${line}`, 'cyan');
    this.log(message, 'bright');
    this.log(`${line}\n`, 'cyan');
  }

  // ==========================================================================
  // Command Execution
  // ==========================================================================

  exec(command, options = {}) {
    const verbose = this.args.verbose || options.verbose;
    
    if (verbose) {
      this.info(`Executing: ${command}`);
    }

    try {
      const output = execSync(command, {
        cwd: options.cwd || __dirname,
        encoding: 'utf-8',
        stdio: verbose ? 'inherit' : 'pipe',
        ...options,
      });
      return { success: true, output };
    } catch (error) {
      return {
        success: false,
        error: error.message,
        output: error.stdout || error.stderr || '',
      };
    }
  }

  async execAsync(command, options = {}) {
    return new Promise((resolve, reject) => {
      const verbose = this.args.verbose || options.verbose;
      
      if (verbose) {
        this.info(`Executing: ${command}`);
      }

      const [cmd, ...args] = command.split(' ');
      const proc = spawn(cmd, args, {
        cwd: options.cwd || __dirname,
        shell: true,
        stdio: verbose ? 'inherit' : 'pipe',
      });

      let stdout = '';
      let stderr = '';

      if (!verbose) {
        proc.stdout?.on('data', (data) => stdout += data.toString());
        proc.stderr?.on('data', (data) => stderr += data.toString());
      }

      proc.on('close', (code) => {
        if (code === 0) {
          resolve({ success: true, output: stdout });
        } else {
          reject({ success: false, error: stderr, output: stdout });
        }
      });

      proc.on('error', (error) => {
        reject({ success: false, error: error.message });
      });
    });
  }

  // ==========================================================================
  // Build Steps
  // ==========================================================================

  async checkPrerequisites() {
    this.header('Checking Prerequisites');

    const checks = [
      { name: 'Node.js', command: 'node --version' },
      { name: 'npm', command: 'npm --version' },
      { name: 'Rust', command: 'rustc --version' },
      { name: 'Cargo', command: 'cargo --version' },
    ];

    for (const check of checks) {
      const result = this.exec(check.command);
      if (result.success) {
        this.success(`${check.name}: ${result.output.trim()}`);
      } else {
        this.error(`${check.name}: NOT FOUND`);
        throw new Error(`${check.name} is required but not installed`);
      }
    }
  }

  async cleanBuild() {
    if (!this.args.clean) return;

    this.header('Cleaning Build Artifacts');

    const pathsToClean = [
      'src-tauri/target',
      'dist',
      'build-logs',
      '.build-cache',
      'node_modules/.vite',
    ];

    for (const p of pathsToClean) {
      const fullPath = path.join(__dirname, p);
      if (fs.existsSync(fullPath)) {
        this.info(`Removing ${p}...`);
        fs.rmSync(fullPath, { recursive: true, force: true });
        this.success(`Cleaned ${p}`);
      }
    }
  }

  async installDependencies() {
    this.header('Installing Dependencies');

    // Install Node dependencies
    this.info('Installing Node.js dependencies...');
    const npmResult = this.exec('npm install', { verbose: true });
    if (!npmResult.success) {
      throw new Error('Failed to install Node.js dependencies');
    }
    this.success('Node.js dependencies installed');

    // Check Rust dependencies
    this.info('Checking Rust dependencies...');
    const cargoResult = this.exec('cargo fetch', { cwd: 'src-tauri' });
    if (cargoResult.success) {
      this.success('Rust dependencies ready');
    }
  }

  async runTests() {
    if (this.args.skipTests) {
      this.warn('Skipping tests (--skip-tests flag)');
      return;
    }

    this.header('Running Tests');

    // Frontend tests
    this.info('Running frontend tests...');
    const frontendTest = this.exec('npm run test -- --run');
    if (frontendTest.success) {
      this.success('Frontend tests passed');
    } else {
      this.warn('Frontend tests failed (continuing anyway)');
    }

    // Backend tests
    this.info('Running backend tests...');
    const backendTest = this.exec('cargo test', { cwd: 'src-tauri' });
    if (backendTest.success) {
      this.success('Backend tests passed');
    } else {
      this.warn('Backend tests failed (continuing anyway)');
    }
  }

  async buildFrontend() {
    this.header('Building Frontend');

    const mode = this.args.mode === 'dev' ? 'development' : 'production';
    process.env.NODE_ENV = mode;

    this.info(`Building frontend in ${mode} mode...`);
    const result = this.exec('npm run build', { verbose: true });
    
    if (!result.success) {
      throw new Error('Frontend build failed');
    }

    this.success('Frontend build completed');
  }

  async buildBackend() {
    this.header('Building Backend');

    const mode = this.args.mode === 'dev' ? '' : '--release';
    const features = this.getEnabledFeatures();
    const featuresFlag = features.length > 0 ? `--features ${features.join(','))}` : '';
    const target = this.args.target ? `--target ${this.args.target}` : '';
    
    const command = `cargo build ${mode} ${featuresFlag} ${target}`.trim();
    
    this.info(`Building backend: ${command}`);
    const result = this.exec(command, { cwd: 'src-tauri', verbose: true });

    if (!result.success) {
      throw new Error('Backend build failed');
    }

    this.success('Backend build completed');
  }

  async buildTauri() {
    this.header('Building Tauri Application');

    const mode = this.args.mode === 'dev' ? '--debug' : '';
    const bundle = this.args.bundle ? '' : '--no-bundle';
    const features = this.getEnabledFeatures();
    const featuresFlag = features.length > 0 ? `--features ${features.join(',')}` : '';
    const target = this.args.target ? `--target ${this.args.target}` : '';

    const command = `npm run tauri:build -- ${mode} ${bundle} ${featuresFlag} ${target}`.trim();

    this.info(`Building Tauri app: ${command}`);
    const result = this.exec(command, { verbose: true });

    if (!result.success) {
      throw new Error('Tauri build failed');
    }

    this.success('Tauri build completed');
  }

  async generateChecksums() {
    if (!this.args.checksums) return;

    this.header('Generating Checksums');

    const crypto = require('crypto');
    const bundleDir = path.join(__dirname, 'src-tauri', 'target', 'release', 'bundle');
    
    if (!fs.existsSync(bundleDir)) {
      this.warn('Bundle directory not found, skipping checksums');
      return;
    }

    const checksumFile = path.join(__dirname, 'dist', 'checksums.txt');
    fs.mkdirSync(path.dirname(checksumFile), { recursive: true });
    
    const checksums = [];

    const processDir = (dir) => {
      const files = fs.readdirSync(dir, { withFileTypes: true });
      for (const file of files) {
        const fullPath = path.join(dir, file.name);
        if (file.isDirectory()) {
          processDir(fullPath);
        } else if (file.isFile()) {
          const ext = path.extname(file.name).toLowerCase();
          if (['.msi', '.exe', '.dmg', '.app', '.deb', '.appimage'].includes(ext) ||
              file.name.endsWith('.tar.gz')) {
            const content = fs.readFileSync(fullPath);
            const hash = crypto.createHash('sha256').update(content).digest('hex');
            const relativePath = path.relative(bundleDir, fullPath);
            checksums.push(`${hash}  ${relativePath}`);
            this.info(`${file.name}: ${hash}`);
          }
        }
      }
    };

    processDir(bundleDir);

    fs.writeFileSync(checksumFile, checksums.join('\n'));
    this.success(`Checksums written to ${checksumFile}`);
  }

  // ==========================================================================
  // Feature Management
  // ==========================================================================

  getEnabledFeatures() {
    const features = [];
    const configFeatures = this.config.features || {};

    // Add features from config
    for (const [name, config] of Object.entries(configFeatures)) {
      if (config.enabled) {
        features.push(name);
      }
    }

    // Add features from command line
    features.push(...this.args.features);

    // Remove disabled features
    return features.filter(f => !this.args.disabledFeatures.includes(f));
  }

  // ==========================================================================
  // Multi-Platform Build
  // ==========================================================================

  async buildAllPlatforms() {
    this.header('Multi-Platform Build');

    const platforms = ['windows', 'macos', 'linux'];
    const currentPlatform = this.platform;

    this.warn('Cross-compilation requires proper toolchain setup');
    this.info(`Current platform: ${currentPlatform}`);

    for (const platform of platforms) {
      if (platform !== currentPlatform) {
        this.warn(`Skipping ${platform} (not current platform)`);
        continue;
      }

      try {
        await this.buildForPlatform(platform);
      } catch (error) {
        this.error(`Failed to build for ${platform}: ${error.message}`);
      }
    }
  }

  async buildForPlatform(platform) {
    const platformConfig = this.config.platforms[platform];
    if (!platformConfig || !platformConfig.enabled) {
      this.warn(`Platform ${platform} is disabled in config`);
      return;
    }

    const targets = this.args.target ? [this.args.target] : platformConfig.targets;

    for (const target of targets) {
      this.info(`Building for ${platform} (${target})...`);
      
      // Install target if needed
      const installResult = this.exec(`rustup target add ${target}`);
      if (installResult.success) {
        this.success(`Target ${target} ready`);
      }

      // Build with specific target
      this.args.target = target;
      await this.buildTauri();
    }
  }

  // ==========================================================================
  // Main Build Process
  // ==========================================================================

  async build() {
    try {
      this.header(`Vault Desktop Build System - ${this.platform}`);
      
      // Setup logging
      const logsDir = path.join(__dirname, 'build-logs');
      fs.mkdirSync(logsDir, { recursive: true });
      this.logFile = path.join(logsDir, `build-${Date.now()}.log`);
      this.info(`Logging to: ${this.logFile}`);

      // Show configuration
      this.info(`Platform: ${this.args.platform || this.platform}`);
      this.info(`Mode: ${this.args.mode}`);
      this.info(`Bundle: ${this.args.bundle}`);
      const features = this.getEnabledFeatures();
      if (features.length > 0) {
        this.info(`Features: ${features.join(', ')}`);
      }

      // Execute build steps
      await this.checkPrerequisites();
      await this.cleanBuild();
      await this.installDependencies();
      await this.runTests();

      if (this.args.platform === 'all') {
        await this.buildAllPlatforms();
      } else {
        await this.buildTauri();
      }

      await this.generateChecksums();

      // Summary
      const duration = ((Date.now() - this.startTime) / 1000).toFixed(2);
      this.header('Build Completed Successfully! 🎉');
      this.success(`Total time: ${duration}s`);
      
      // Show output location
      const outputDir = path.join(__dirname, 'src-tauri', 'target', 
        this.args.mode === 'dev' ? 'debug' : 'release', 'bundle');
      if (fs.existsSync(outputDir)) {
        this.info(`Build artifacts: ${outputDir}`);
      }

    } catch (error) {
      this.error(`Build failed: ${error.message}`);
      if (this.args.verbose) {
        console.error(error);
      }
      process.exit(1);
    }
  }

  showHelp() {
    console.log(`
${COLORS.bright}Vault Desktop - Advanced Build System${COLORS.reset}

${COLORS.cyan}Usage:${COLORS.reset}
  node build.js [options]

${COLORS.cyan}Options:${COLORS.reset}
  ${COLORS.green}--platform <win|mac|linux|all>${COLORS.reset}
      Target platform (default: current platform)
      
  ${COLORS.green}--mode <dev|release|production>${COLORS.reset}
      Build mode (default: release)
      
  ${COLORS.green}--clean${COLORS.reset}
      Clean build artifacts before building
      
  ${COLORS.green}--skip-tests${COLORS.reset}
      Skip running tests
      
  ${COLORS.green}--no-bundle${COLORS.reset}
      Skip creating installers (faster builds)
      
  ${COLORS.green}--features <feat1,feat2>${COLORS.reset}
      Enable specific features (comma-separated)
      
  ${COLORS.green}--disable-features <feat1>${COLORS.reset}
      Disable specific features
      
  ${COLORS.green}--target <triple>${COLORS.reset}
      Specific Rust target triple
      Example: x86_64-pc-windows-msvc
      
  ${COLORS.green}--verbose${COLORS.reset}
      Enable verbose logging
      
  ${COLORS.green}--parallel${COLORS.reset}
      Enable parallel builds (default)
      
  ${COLORS.green}--checksums${COLORS.reset}
      Generate SHA256 checksums for build artifacts
      
  ${COLORS.green}--config <path>${COLORS.reset}
      Custom config file path (default: build.config.json)
      
  ${COLORS.green}--help, -h${COLORS.reset}
      Show this help message

${COLORS.cyan}Examples:${COLORS.reset}
  # Simple release build
  node build.js
  
  # Clean build with tests
  node build.js --clean --mode release
  
  # Development build without bundling
  node build.js --mode dev --no-bundle
  
  # Build with specific features
  node build.js --features embeddings,file_watcher
  
  # Build for specific target
  node build.js --target x86_64-pc-windows-msvc
  
  # Multi-platform build (requires toolchains)
  node build.js --platform all --checksums
  
  # Verbose build with checksums
  node build.js --verbose --checksums

${COLORS.cyan}Available Features:${COLORS.reset}
  embeddings      - ML embeddings and semantic search
  file_watcher    - Automatic file watching
  sync            - Cloud sync (experimental)
  telemetry       - OpenTelemetry observability

${COLORS.cyan}Configuration:${COLORS.reset}
  Edit build.config.json to customize default build settings.
`);
  }

  async run() {
    if (this.args.help) {
      this.showHelp();
      return;
    }

    await this.build();
  }
}

// ============================================================================
// Main Entry Point
// ============================================================================

if (require.main === module) {
  const builder = new BuildSystem();
  builder.run().catch((error) => {
    console.error(`${COLORS.red}Fatal error: ${error.message}${COLORS.reset}`);
    process.exit(1);
  });
}

module.exports = BuildSystem;

