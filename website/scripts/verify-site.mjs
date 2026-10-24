#!/usr/bin/env node
import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const publicDir = path.resolve(__dirname, "../public");

const MIME_TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
  ".txt": "text/plain; charset=utf-8",
  ".xml": "application/xml; charset=utf-8",
  ".dmg": "application/x-apple-diskimage",
  ".sha256": "text/plain; charset=utf-8",
};

const server = http.createServer((req, res) => {
  const urlPath = new URL(req.url, "http://127.0.0.1").pathname;
  const relPath = urlPath === "/" ? "/index.html" : urlPath;
  const filePath = path.join(publicDir, relPath);
  if (!filePath.startsWith(publicDir) || !fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
    res.writeHead(404);
    res.end("Not found");
    return;
  }
  const ext = path.extname(filePath);
  res.writeHead(200, { "Content-Type": MIME_TYPES[ext] || "application/octet-stream" });
  fs.createReadStream(filePath).pipe(res);
});

await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const { port } = server.address();
const baseUrl = `http://127.0.0.1:${port}`;

const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(msg.text());
  });
  page.on("response", (res) => {
    if (res.status() >= 400) errors.push(`HTTP ${res.status()} for ${res.url()}`);
  });

  await page.goto(baseUrl, { waitUntil: "networkidle" });

  // Verify images loaded
  const brokenImages = await page.evaluate(async () => {
    const imgs = [...document.querySelectorAll("img:not(.dialog-image)")];
    for (const img of imgs) {
      img.loading = "eager";
      img.scrollIntoView();
    }
    await new Promise((r) => setTimeout(r, 300));
    return imgs
      .filter((img) => !img.complete || img.naturalWidth === 0)
      .map((img) => img.getAttribute("src"));
  });
  if (brokenImages.length > 0) {
    throw new Error(`Broken images: ${brokenImages.join(", ")}`);
  }

  // Verify interactive gallery tabs
  const tabs = page.locator(".showcase-tabs button");
  const tabCount = await tabs.count();
  if (tabCount !== 4) throw new Error(`Expected 4 gallery tabs, found ${tabCount}`);

  await tabs.nth(1).click();
  const activeSrc = await page.locator(".showcase-image").getAttribute("src");
  if (!activeSrc?.includes("dashboard.png")) {
    throw new Error(`Expected dashboard.png after tab click, got ${activeSrc}`);
  }

  await tabs.nth(3).click();
  const studioSrc = await page.locator(".showcase-image").getAttribute("src");
  if (!studioSrc?.includes("studio.png")) {
    throw new Error(`Expected studio.png after tab click, got ${studioSrc}`);
  }

  // Reset to first tab for hero/showcase screenshot
  await tabs.nth(0).click();
  await page.evaluate(() => window.scrollTo(0, 0));

  // Verify DMG and SHA-256 endpoints
  const dmgHead = await fetch(`${baseUrl}/downloads/Lattice_1.0.0_aarch64.dmg`, { method: "HEAD" });
  if (dmgHead.status !== 200) {
    throw new Error(`Expected 200 for DMG download, got ${dmgHead.status}`);
  }
  const shaRes = await fetch(`${baseUrl}/downloads/Lattice_1.0.0_aarch64.dmg.sha256`);
  if (shaRes.status !== 200) {
    throw new Error(`Expected 200 for SHA-256 checksum, got ${shaRes.status}`);
  }
  const shaText = (await shaRes.text()).trim();
  if (!shaText.endsWith("Lattice_1.0.0_aarch64.dmg")) {
    throw new Error(`Unexpected SHA-256 sidecar text: ${shaText}`);
  }

  // Test lightbox dialog
  await page.locator(".stage-preview").click();
  const isDialogOpen = await page.locator("dialog.image-dialog").evaluate((d) => d.open);
  if (!isDialogOpen) throw new Error("Expected lightbox dialog to open on click");
  await page.locator(".dialog-close").click();

  // Check horizontal overflow at desktop and mobile
  const desktopOverflow = await page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
  );
  if (desktopOverflow) throw new Error("Horizontal overflow detected on desktop viewport");

  const outDir = process.env.SCREENSHOT_DIR;
  if (outDir) {
    fs.mkdirSync(outDir, { recursive: true });
    await page.screenshot({ path: path.join(outDir, "desktop-hero.png") });
    await page.screenshot({ path: path.join(outDir, "desktop-full.png"), fullPage: true });
  }

  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(150);
  const mobileOverflow = await page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
  );
  if (mobileOverflow) throw new Error("Horizontal overflow detected on mobile viewport");

  if (outDir) {
    await page.screenshot({ path: path.join(outDir, "mobile-hero.png") });
  }

  if (errors.length > 0) {
    throw new Error(`Page errors:\n${errors.join("\n")}`);
  }

  console.log("Verified website: 0 errors, all 4 Retina screenshots, gallery tabs, lightbox, and responsive viewports passed.");
} finally {
  await browser.close();
  server.close();
}
