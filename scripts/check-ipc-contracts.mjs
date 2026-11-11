#!/usr/bin/env node
/**
 * Self-test for the typed IPC contract. `apiCall` takes its arguments and result from the generated
 * `CommandArgs` and `CommandResults`, so tsc checks every call site. This compiles probe files of stale
 * usage and fails unless tsc rejects each one for the reason named.
 */
import ts from '../node_modules/typescript/lib/typescript.js';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// TypeScript names files with forward slashes on every OS, while Node's path
// joins with backslashes on Windows.
const tsPath = file => path.sep === '\\' ? file.replaceAll('\\', '/') : file;

/** Compile `source` as `probePath` inside the project at `tsconfig`; return the diagnostics by 1-based line. */
function diagnosticsByLine(tsconfig, probePath, source) {
    const config = ts.readConfigFile(path.join(root, tsconfig), ts.sys.readFile);
    const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
    const probe = tsPath(probePath);
    const host = ts.createCompilerHost(parsed.options);
    const getSourceFile = host.getSourceFile.bind(host);
    host.getSourceFile = (file, ...args) => file === probe
        ? ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true)
        : getSourceFile(file, ...args);
    const program = ts.createProgram([...parsed.fileNames, probe], parsed.options, host);
    const file = program.getSourceFile(probe);
    const byLine = new Map();
    for (const diagnostic of ts.getPreEmitDiagnostics(program, file)) {
        const line = diagnostic.start === undefined ? 0 : file.getLineAndCharacterOfPosition(diagnostic.start).line + 1;
        byLine.set(line, [...(byLine.get(line) ?? []), ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')]);
    }
    return byLine;
}

const header = [
    "import { Channel } from '@tauri-apps/api/core';",
    "import type { GenerateLearningProgramRequestDto, LearningOutlineProgressDto } from './lib/bindings';",
    "import { apiCall } from './shared/ipc/transport';",
    'declare const outlineRequest: GenerateLearningProgramRequestDto;',
];

/** Each probe is one line of source, the text tsc's message must contain, and why. `needle: null` means tsc must accept it. */
const srcProbes = [
    { why: 'a valid call with arguments', needle: null, code: "void apiCall('delete_model', { modelId: 'model', deleteFile: true });" },
    { why: 'a valid call with no arguments', needle: null, code: "void apiCall('list_study_decks');" },
    { why: 'a valid call with a channel', needle: null, code: "void apiCall('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: new Channel<LearningOutlineProgressDto>() });" },
    { why: 'an unknown command', needle: 'command_without_a_generated_contract', code: "void apiCall('command_without_a_generated_contract', {});" },
    { why: 'a nested request field the command does not have', needle: 'document_id', code: "void apiCall('remove_tag_from_document', { request: { document_id: 'doc', tag_id: 'tag' } });" },
    { why: 'a missing required argument', needle: 'deleteFile', code: "void apiCall('delete_model', { modelId: 'model' });" },
    { why: 'a channel carrying the wrong payload', needle: 'Channel<string>', code: "void apiCall('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: new Channel<string>() });" },
    { why: 'a plain object where a channel is required', needle: 'Channel', code: "void apiCall('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: {} });" },
    { why: 'null where a channel is required', needle: 'Channel', code: "void apiCall('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: null });" },
    { why: 'a field the result lacks', needle: 'madeUpField', code: "export async function readResult() { const r = await apiCall('list_study_decks'); if (r.ok) r.data.map(deck => deck.madeUpField); }" },
];

const srcHeader = header;
const srcSource = [...srcHeader, ...srcProbes.map(probe => probe.code)].join('\n');
const srcDiagnostics = diagnosticsByLine('tsconfig.json', path.join(root, 'src', '__contract_probe__.ts'), srcSource);
const failures = [];
srcProbes.forEach((probe, index) => {
    const messages = srcDiagnostics.get(srcHeader.length + index + 1) ?? [];
    if (probe.needle === null && messages.length)
        failures.push(`tsc rejects ${probe.why}: ${messages.join(' | ')}`);
    if (probe.needle !== null && !messages.some(message => message.includes(probe.needle)))
        failures.push(`tsc no longer rejects ${probe.why} (expected a message naming ${probe.needle}; got ${messages.length ? messages.join(' | ') : 'none'})`);
});
for (const line of srcDiagnostics.keys())
    if (line <= srcHeader.length)
        failures.push(`tsc rejects the probe header (line ${line}): ${srcDiagnostics.get(line).join(' | ')}`);
if (failures.length) {
    for (const failure of failures) console.error(failure);
    throw new Error('IPC guard failed to detect intentionally broken contracts');
}
console.log('apiCall rejects unknown commands, unexpected nested request fields, missing arguments, invalid channels and fields a result lacks, and accepts valid calls.');

// Browser e2e fixtures answer IPC through e2e/fixtures/tauri.ts, typed by the
// same generated contracts; `tsc -p tsconfig.e2e.json` is what rejects a stale
// fixture. Prove the harness still makes it reject one.
const e2eProbe = `
import type { Page } from '@playwright/test';
import { mockCommands, serveCommands } from './fixtures/tauri';
declare const page: Page;
export async function probe() {
    await mockCommands(page, () => ({ list_conversation_spaces: () => [{ id: 'space', name: 'General', isArchivedd: false }] }), undefined);
    await mockCommands(page, () => ({ list_journals: () => [], command_without_a_generated_contract: () => null }), undefined);
    await mockCommands(page, () => ({ get_learning_program: () => ({ modules: [{ lessons: [{ blocks: [{ kind: 'explanation' as const, titl: 'x' }] }] }] }) }), undefined);
    await serveCommands(page, () => ({ get_indexing_activities: () => [{ id: 'activity', madeUpField: 1 }] }));
}
`;
const rejected = [...diagnosticsByLine('tsconfig.e2e.json', path.join(root, 'e2e', '__contract_probe__.ts'), e2eProbe).values()].flat().join('\n');
for (const expected of ['"isArchivedd"', '"command_without_a_generated_contract"', '"modules.lessons.blocks.titl"', '"madeUpField"'])
    if (!rejected.includes(expected))
        throw new Error(`e2e IPC fixtures no longer reject ${expected}`);
console.log('e2e IPC fixtures reject unknown commands and fields the generated DTOs lack.');
