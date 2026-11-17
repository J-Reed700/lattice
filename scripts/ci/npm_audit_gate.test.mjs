import assert from "node:assert/strict";
import test from "node:test";
import { auditFailures } from "./npm_audit_gate.mjs";

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

const picomatchOnly = () =>
  audit({
    picomatch: vulnerability("picomatch", "high", [advisory("picomatch", PICOMATCH, "high")], { name: "vitest", version: "3.1.4", isSemVerMajor: true }, ["vitest"]),
    vitest: vulnerability("vitest", "high", ["picomatch"], { name: "vitest", version: "3.1.4", isSemVerMajor: true }),
  });

test("an allow-listed advisory with no fix passes, even when npm offers a major downgrade", () => {
  assert.deepEqual(auditFailures(picomatchOnly(), allow), []);
});

test("a high advisory that is not allow-listed fails by its GHSA id", () => {
  const failures = auditFailures(picomatchOnly(), []);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /GHSA-fc5q-39xc-vjpf/);
});

test("a critical advisory that is not allow-listed fails", () => {
  const report = audit({ left: vulnerability("left", "critical", [advisory("left", "GHSA-aaaa-bbbb-cccc", "critical")]) });
  const failures = auditFailures(report, []);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /GHSA-aaaa-bbbb-cccc/);
});

test("moderate and low advisories pass", () => {
  const report = audit({
    mod: vulnerability("mod", "moderate", [advisory("mod", "GHSA-1111-2222-3333", "moderate")]),
    low: vulnerability("low", "low", [advisory("low", "GHSA-4444-5555-6666", "low")]),
  });
  assert.deepEqual(auditFailures(report, []), []);
});

test("an allow-listed advisory fails once its own entry has a fix", () => {
  const report = picomatchOnly();
  report.vulnerabilities.picomatch.fixAvailable = true;
  const failures = auditFailures(report, allow);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /GHSA-fc5q-39xc-vjpf.*fix available/);
});

test("a fix flag on a dependent of the advisory does not count, since npm sets it without any change to install", () => {
  const report = picomatchOnly();
  report.vulnerabilities.fdir = vulnerability("fdir", "high", ["picomatch"], true);
  assert.deepEqual(auditFailures(report, allow), []);
});

test("a fix on an unrelated vulnerability does not count", () => {
  const report = picomatchOnly();
  report.vulnerabilities.other = vulnerability("other", "moderate", [advisory("other", "GHSA-7777-8888-9999", "moderate")], true);
  assert.deepEqual(auditFailures(report, allow), []);
});

test("an allow-listed advisory that is no longer reported fails", () => {
  const failures = auditFailures(audit({}), allow);
  assert.equal(failures.length, 1);
  assert.match(failures[0], /no longer reported.*remove it from/);
});
