import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const BLOCKING = new Set(["high", "critical"]);
const SAFE_NAME = /^(?:@[a-z0-9][a-z0-9._~-]*\/)?[a-z0-9][a-z0-9._~-]*$/i;
const SAFE_RANGE = /^[0-9a-z.<>=~^*|\s-]+$/i;

export function advisoryId(url) {
  const match = /GHSA-[0-9a-z]{4}-[0-9a-z]{4}-[0-9a-z]{4}/i.exec(url ?? "");
  return match ? match[0] : url;
}

function realFix(fixAvailable) {
  if (fixAvailable === true) return true;
  return Boolean(fixAvailable) && typeof fixAvailable === "object" && fixAvailable.isSemVerMajor === false;
}

function reported(audit) {
  const found = new Map();
  for (const entry of Object.values(audit.vulnerabilities ?? {})) {
    for (const via of entry.via ?? []) {
      if (typeof via === "string") continue;
      const id = advisoryId(via.url);
      if (!found.has(id)) {
        found.set(id, { id, package: via.name, severity: via.severity, title: via.title, range: via.range, owner: entry.name });
      }
    }
  }
  return found;
}

function publishedVersions(spec) {
  try {
    return [JSON.parse(execSync(`npm view "${spec}" version --json`, { encoding: "utf8" }))].flat();
  } catch (error) {
    if (JSON.parse(error.stdout || "{}").error?.code === "E404") return [];
    throw error;
  }
}

export function patchedReleaseExists(name, range, versions = publishedVersions) {
  if (!SAFE_NAME.test(name) || !SAFE_RANGE.test(range)) {
    throw new Error(`npm audit reported an unexpected package or range: ${JSON.stringify(name)} ${JSON.stringify(range)}`);
  }
  const [latest] = versions(name);
  return !versions(`${name}@${range}`).includes(latest);
}

export function auditFailures(audit, allowList, patched) {
  if (!audit.vulnerabilities) return [`npm audit gave no report: ${audit.error?.summary ?? JSON.stringify(audit)}`];
  const failures = [];
  const found = reported(audit);
  const allowed = new Map(allowList.map((entry) => [entry.id, entry]));
  const vulnerabilities = audit.vulnerabilities;

  for (const advisory of found.values()) {
    if (BLOCKING.has(advisory.severity) && !allowed.has(advisory.id)) {
      failures.push(
        `${advisory.id} (${advisory.severity}, ${advisory.package}): ${advisory.title}. Fix it, or add it to scripts/ci/npm-audit-allow.json if no fixed release exists.`,
      );
    }
  }

  for (const entry of allowList) {
    const advisory = found.get(entry.id);
    if (!advisory) {
      failures.push(`${entry.id} (${entry.package}) is no longer reported: remove it from scripts/ci/npm-audit-allow.json.`);
      continue;
    }
    if (advisory.package !== entry.package) {
      failures.push(`${entry.id} is reported for ${advisory.package}, not ${entry.package}: correct its entry in scripts/ci/npm-audit-allow.json.`);
      continue;
    }
    if (realFix(vulnerabilities[advisory.owner]?.fixAvailable) || patched(advisory.package, advisory.range)) {
      failures.push(`${entry.id} (${entry.package}) now has a fix available: update the dependency and remove it from scripts/ci/npm-audit-allow.json.`);
    }
  }
  return failures;
}

function main() {
  const allowPath = process.argv[2] ?? new URL("./npm-audit-allow.json", import.meta.url);
  const allowList = JSON.parse(readFileSync(allowPath, "utf8"));
  let output;
  try {
    output = execSync("npm audit --json", { encoding: "utf8", maxBuffer: 1 << 28 });
  } catch (error) {
    output = error.stdout;
  }
  const failures = auditFailures(JSON.parse(output || "{}"), allowList, patchedReleaseExists);
  for (const failure of failures) console.error(failure);
  if (failures.length > 0) process.exit(1);
  console.log("npm audit gate passed.");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
