import assert from "node:assert/strict";
import test from "node:test";
import { auditFailures, patchedReleaseExists } from "./npm_audit_gate.mjs";

const PICOMATCH = "GHSA-fc5q-39xc-vjpf";

function advisory(name, id, severity) {
  return {
    source: 1,
    name,
    dependency: name,
    title: `${name} advisory`,
    url: `https://github.com/advisories/${id}`,
    severity,
    range: "<=1.0.0",
  };
}

function vulnerability(name, severity, via, fixAvailable = false, effects = []) {
  return { name, severity, isDirect: false, via, effects, range: "*", nodes: [`node_modules/${name}`], fixAvailable };
}

function audit(vulnerabilities) {
  return { auditReportVersion: 2, vulnerabilities, metadata: {} };
}

const allow = [{ id: PICOMATCH, package: "picomatch", reason: "No fix.", added: "2026-11-17" }];
const noPatch = () => false;

const picomatchOnly = () =>
  audit({
    picomatch: vulnerability("picomatch", "high", [advisory("picomatch", PICOMATCH, "high")], { name: "vitest", version: "3.1.4", isSemVerMajor: true }, ["vitest"]),
    vitest: vulnerability("vitest", "high", ["picomatch"], { name: "vitest", version: "3.1.4", isSemVerMajor: true }),
  });

test("an allow-listed advisory with no fix passes, even when npm offers a major downgrade", () => {
  assert.deepEqual(auditFailures(picomatchOnly(), allow, noPatch), []);
});

test("a high advisory that is not allow-listed fails by its GHSA id", () => {
  const failures = auditFailures(picomatchOnly(), [], noPatch);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /GHSA-fc5q-39xc-vjpf/);
});

test("a critical advisory that is not allow-listed fails", () => {
  const report = audit({ left: vulnerability("left", "critical", [advisory("left", "GHSA-aaaa-bbbb-cccc", "critical")]) });
  const failures = auditFailures(report, [], noPatch);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /GHSA-aaaa-bbbb-cccc/);
});

test("moderate and low advisories pass", () => {
  const report = audit({
    mod: vulnerability("mod", "moderate", [advisory("mod", "GHSA-1111-2222-3333", "moderate")]),
    low: vulnerability("low", "low", [advisory("low", "GHSA-4444-5555-6666", "low")]),
  });
  assert.deepEqual(auditFailures(report, [], noPatch), []);
});

test("an allow-listed advisory fails once its own entry has a fix", () => {
  const report = picomatchOnly();
  report.vulnerabilities.picomatch.fixAvailable = true;
  const failures = auditFailures(report, allow, noPatch);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /GHSA-fc5q-39xc-vjpf.*fix available/);
});

test("a fix flag on a dependent of the advisory does not count, since npm sets it without any change to install", () => {
  const report = picomatchOnly();
  report.vulnerabilities.fdir = vulnerability("fdir", "high", ["picomatch"], true);
  assert.deepEqual(auditFailures(report, allow, noPatch), []);
});

test("a fix on an unrelated vulnerability does not count", () => {
  const report = picomatchOnly();
  report.vulnerabilities.other = vulnerability("other", "moderate", [advisory("other", "GHSA-7777-8888-9999", "moderate")], true);
  assert.deepEqual(auditFailures(report, allow, noPatch), []);
});

test("an allow-listed advisory that is no longer reported fails", () => {
  const failures = auditFailures(audit({}), allow, noPatch);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /no longer reported.*remove it from/);
});

test("an allow-listed advisory fails once a release outside its vulnerable range is published", () => {
  const patched = (name, range) => name === "picomatch" && range === "<=1.0.0";
  const failures = auditFailures(picomatchOnly(), allow, patched);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /GHSA-fc5q-39xc-vjpf.*fix available/);
});

test("an audit that gave no report fails with npm's own error", () => {
  const failures = auditFailures({ error: { code: "ENOTFOUND", summary: "request to the registry failed" } }, allow, noPatch);
  assert.deepEqual(failures, ["npm audit gave no report: request to the registry failed"]);
});

const published = (latest, inRange) => (spec) => (spec.includes("@", 1) ? inRange : [latest]);

test("a package whose latest release is still in the vulnerable range has no patched release", () => {
  assert.equal(patchedReleaseExists("picomatch", "<=4.0.7", published("4.0.7", ["4.0.6", "4.0.7"])), false);
});

test("a package whose latest release is outside the vulnerable range has a patched release", () => {
  assert.equal(patchedReleaseExists("picomatch", "<=4.0.7", published("4.0.8", ["4.0.6", "4.0.7"])), true);
});

test("a range that matches no published release leaves the latest release unaffected", () => {
  assert.equal(patchedReleaseExists("picomatch", ">=9.0.0 <=14.0.1", published("4.0.7", [])), true);
});

test("advisory data that would change the npm command is refused", () => {
  assert.throws(() => patchedReleaseExists("picomatch", '<=4.0.7" & echo', published("4.0.7", ["4.0.7"])), /unexpected/);
  assert.throws(() => patchedReleaseExists("pico match", "<=4.0.7", published("4.0.7", ["4.0.7"])), /unexpected/);
});
