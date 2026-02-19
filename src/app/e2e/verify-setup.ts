#!/usr/bin/env node

import fs from 'fs';
import path from 'path';

const REQUIRED_FILES = [
  'playwright.config.ts',
  'e2e/setup.ts',
  'e2e/README.md',
  'e2e/QUICK_START.md',
  'e2e/flows/upload-search.spec.ts',
  'e2e/flows/watch-folder.spec.ts',
  'e2e/flows/settings.spec.ts',
  'e2e/flows/onboarding.spec.ts',
  'e2e/pages/BasePage.ts',
  'e2e/pages/SearchPage.ts',
  'e2e/pages/UploadPage.ts',
  'e2e/pages/SettingsPage.ts',
  'e2e/pages/WatchFolderPage.ts',
  'e2e/pages/OnboardingPage.ts',
  'e2e/pages/index.ts',
  'e2e/utils/test-helpers.ts',
  'e2e/fixtures/test-document.txt',
  'e2e/fixtures/sample-notes.md',
  'e2e/fixtures/code-sample.txt',
];

const REQUIRED_PACKAGES = [
  '@playwright/test',
  'playwright',
];

console.log('🔍 Verifying E2E Test Setup...\n');

let allGood = true;

console.log('📁 Checking required files...');
REQUIRED_FILES.forEach((file) => {
  const filePath = path.resolve(process.cwd(), file);
  const exists = fs.existsSync(filePath);
  const status = exists ? '✅' : '❌';
  console.log(`  ${status} ${file}`);
  if (!exists) allGood = false;
});

console.log('\n📦 Checking required packages...');
const packageJsonPath = path.resolve(process.cwd(), 'package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
const devDeps = packageJson.devDependencies || {};

REQUIRED_PACKAGES.forEach((pkg) => {
  const installed = pkg in devDeps;
  const status = installed ? '✅' : '❌';
  const version = installed ? devDeps[pkg] : 'NOT FOUND';
  console.log(`  ${status} ${pkg} (${version})`);
  if (!installed) allGood = false;
});

console.log('\n📝 Checking npm scripts...');
const scripts = packageJson.scripts || {};
const requiredScripts = [
  'test:e2e',
  'test:e2e:ui',
  'test:e2e:debug',
  'test:e2e:headed',
  'test:e2e:report',
];

requiredScripts.forEach((script) => {
  const exists = script in scripts;
  const status = exists ? '✅' : '❌';
  console.log(`  ${status} ${script}`);
  if (!exists) allGood = false;
});

console.log('\n📊 Test Statistics:');
console.log('  • Test Suites: 4');
console.log('  • Test Cases: ~35');
console.log('  • Page Objects: 6');
console.log('  • Test Fixtures: 3');

console.log('\n🚀 Quick Start:');
console.log('  1. Install Playwright browsers:');
console.log('     npx playwright install chromium');
console.log('\n  2. Run tests:');
console.log('     npm run test:e2e');
console.log('\n  3. Run with UI:');
console.log('     npm run test:e2e:ui');

if (allGood) {
  console.log('\n✅ All checks passed! E2E test setup is complete.');
  console.log('   Read e2e/README.md for detailed documentation.');
  process.exit(0);
} else {
  console.log('\n❌ Some checks failed. Please review the output above.');
  console.log('   Run this script again after fixing the issues.');
  process.exit(1);
}
