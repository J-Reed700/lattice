#!/usr/bin/env node
/** Check IPC wire types against the Rust-generated command signatures. UI view models may differ. */
import ts from '../node_modules/typescript/lib/typescript.js';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const config = ts.readConfigFile(path.join(root, 'tsconfig.json'), ts.sys.readFile);
const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
const probePath = path.join(root, 'src', '__contract_probe__.ts');
const host = ts.createCompilerHost(parsed.options);
const getSourceFile = host.getSourceFile.bind(host);
const probe = `
import { Channel } from '@tauri-apps/api/core';
import type { GenerateLearningProgramRequestDto, LearningProgramDto, LearningOutlineProgressDto } from './lib/bindings';
declare function invoke<T>(command: string, args?: unknown): Promise<T>;
invoke<{ madeUpField: string }[]>('get_indexing_activities', { limit: 10 });
invoke<{ madeUpField: string }[]>('get_learning_runtime_catalog');
invoke<null>('remove_tag_from_document', { request: { documentId: 'doc', tagId: 'tag' } });
invoke<null>('remove_tag_from_document', { request: { document_id: 'doc', tag_id: 'tag' } });
invoke<null>('command_without_a_generated_contract');
invoke<null>('delete_model', { modelId: 'model' });
declare const outlineRequest: GenerateLearningProgramRequestDto;
invoke<LearningProgramDto>('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: new Channel<LearningOutlineProgressDto>() });
invoke<LearningProgramDto>('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: {} });
invoke<LearningProgramDto>('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: null });
invoke<LearningProgramDto>('generate_learning_program', { request: outlineRequest, requestId: null, onProgress: new Channel<string>() });
declare function apiCall<T>(command: string, args?: unknown): Promise<T>;
apiCall<null>('command_without_a_route');
import type { CommandName } from './shared/ipc/transport';
import type { ApiResult } from './types';
declare function featureCall<T>(command: CommandName, args?: Record<string, unknown>): Promise<ApiResult<T>>;
featureCall<{ madeUpField: string }[]>('list_study_decks');
`;
if (process.argv.includes('--self-test')) {
    host.getSourceFile = (file, ...args) => file === probePath
        ? ts.createSourceFile(file, probe, ts.ScriptTarget.Latest, true)
        : getSourceFile(file, ...args);
    parsed.fileNames.push(probePath);
}
const program = ts.createProgram(parsed.fileNames, parsed.options, host), checker = program.getTypeChecker();
function visit(node, callback) { callback(node); ts.forEachChild(node, child => visit(child, callback)); }
const bindings = program.getSourceFile(path.join(root, 'src/lib/bindings.ts')), contracts = new Map();
visit(bindings, node => {
    if (!ts.isMethodDeclaration(node))
        return;
    const awaited = node.type?.typeArguments?.[0];
    // Most commands return Promise<Result<T, ApiError>>, but infallible
    // commands return Promise<T> directly. Preserve arrays and other raw types.
    const response = awaited && ts.isTypeReferenceNode(awaited)
        && awaited.typeName.getText(bindings) === 'Result'
        ? awaited.typeArguments?.[0] : awaited;
    if (!response)
        return;
    visit(node.body, call => {
        if (ts.isCallExpression(call) && call.expression.getText(bindings) === 'TAURI_INVOKE' && ts.isStringLiteral(call.arguments[0]))
            contracts.set(call.arguments[0].text.replace(/^plugin:[^|]+\|/, ''), { type: checker.getTypeFromTypeNode(response), text: response.getText(bindings), params: node.parameters.map(p => ({ name: p.name.getText(bindings), type: checker.getTypeAtLocation(p) })) });
    });
});
// The generated route table: every command the transport can invoke.
const routeTable = program.getSourceFile(path.join(root, 'src/shared/ipc/routes.generated.ts')), routes = new Set();
visit(routeTable, node => {
    if (ts.isVariableDeclaration(node) && node.name.getText(routeTable) === 'COMMAND_PLUGINS')
        for (const p of node.initializer.expression.properties)
            routes.add(p.name.getText(routeTable));
});
// apiCall and the feature wrappers around it take a generated CommandName.
function routedCall(call) {
    if (call.expression.getText(call.getSourceFile()) === 'apiCall')
        return true;
    const declaration = checker.getResolvedSignature(call)?.parameters[0]?.valueDeclaration;
    return declaration !== undefined && ts.isParameter(declaration) && declaration.type !== undefined
        && ts.isTypeReferenceNode(declaration.type) && declaration.type.typeName.getText() === 'CommandName';
}
const report = { generatedCommands: contracts.size, checked: 0, mismatches: [], uncovered: [], unrouted: [], argumentNames: [] };
function nullable(type) { return Boolean(type.flags & (ts.TypeFlags.Null | ts.TypeFlags.Undefined)) || type.isUnion() && type.types.some(nullable); }
function checkObject(expression, expected, context, prefix = '') {
    const optional = nullable(expected);
    expected = checker.getNonNullableType(expected);
    if (!(expected.flags & ts.TypeFlags.Object) || checker.isArrayType(expected) || checker.isTupleType(expected)) return;
    // Transport classes such as Tauri Channel serialize themselves. TypeScript
    // checks their nominal identity and payload type; private members are not
    // wire fields and cannot be looked up by their displayed property names.
    if (expected.getSymbol()?.declarations?.some(ts.isClassDeclaration)) {
        const supplied = checker.getTypeAtLocation(expression);
        const checked = optional ? checker.getNonNullableType(supplied) : supplied;
        if (!checker.isTypeAssignableTo(checked, expected))
            report.argumentNames.push({ ...context, field: prefix.replace(/\.$/, ''), actual: checker.typeToString(supplied), expected: checker.typeToString(expected) });
        return;
    }
    if (checker.getIndexTypeOfType(expected, ts.IndexKind.String))
        return;
    const properties = checker.getPropertiesOfType(expected);
    if (!properties.length)
        return;
    const actual = checker.getNonNullableType(checker.getTypeAtLocation(expression));
    for (const property of properties) {
        if (!(property.flags & ts.SymbolFlags.Optional) && !nullable(checker.getTypeOfSymbolAtLocation(property, expression)) && !checker.getPropertyOfType(actual, property.name))
            report.argumentNames.push({ ...context, missing: prefix + property.name });
    }
    for (const property of checker.getPropertiesOfType(actual)) {
        const match = properties.find(p => p.name === property.name);
        if (!match) {
            report.argumentNames.push({ ...context, unexpected: prefix + property.name, expected: properties.map(p => prefix + p.name) });
            continue;
        }
        const wanted = checker.getTypeOfSymbolAtLocation(match, expression);
        const supplied = checker.getTypeOfSymbolAtLocation(property, expression);
        // Missing/null are equivalent for an optional Rust command argument.
        const checked = nullable(wanted) ? checker.getNonNullableType(supplied) : supplied;
        if (!checker.isTypeAssignableTo(checked, wanted))
            report.argumentNames.push({ ...context, field: prefix + property.name, actual: checker.typeToString(supplied), expected: checker.typeToString(wanted) });
        const declaration = property.valueDeclaration;
        if (declaration && ts.isPropertyAssignment(declaration) && ts.isObjectLiteralExpression(declaration.initializer))
            checkObject(declaration.initializer, wanted, context, prefix + property.name + '.');
    }
}
for (const source of program.getSourceFiles()) {
    if (!source.fileName.startsWith(path.join(root, 'src')) || source === bindings || /(?:__tests__|__mocks__|\.test\.|\.spec\.)/.test(source.fileName))
        continue;
    visit(source, call => {
        if (!ts.isCallExpression(call) || !call.arguments.length || !ts.isStringLiteral(call.arguments[0]))
            return;
        const routed = routedCall(call);
        if (!routed && call.expression.getText(source) !== 'invoke')
            return;
        const inputName = call.arguments[0].text, command = inputName.replace(/^plugin:[^|]+\|/, '');
        const location = `${path.relative(root, source.fileName)}:${source.getLineAndCharacterOfPosition(call.getStart(source)).line + 1}`;
        // A routed call names a generated command; anything else cannot be
        // invoked, so it is a dead feature rather than a fallback.
        if (routed && !routes.has(inputName)) {
            report.unrouted.push({ command: inputName, location });
            return;
        }
        const contract = contracts.get(command);
        let declared = call.typeArguments?.[0] ? checker.getTypeFromTypeNode(call.typeArguments[0]) : checker.getAwaitedType(checker.getTypeAtLocation(call));
        if (!call.typeArguments?.[0] && routed) {
            const data = (declared.isUnion() ? declared.types : [declared]).map(t => checker.getPropertyOfType(t, 'data')).find(Boolean);
            if (data)
                declared = checker.getTypeOfSymbolAtLocation(data, call);
        }
        if (!contract) {
            report.uncovered.push({ command, location, declared: checker.typeToString(declared) });
            return;
        }
        report.checked++;
        if (!checker.isTypeAssignableTo(contract.type, declared) && !(contract.type.flags & ts.TypeFlags.Null && declared.flags & ts.TypeFlags.Void))
            report.mismatches.push({ command, location, declared: checker.typeToString(declared), actual: contract.text });
        if (declared.flags & ts.TypeFlags.Any)
            report.mismatches.push({ command, location, error: 'Unvalidated any at IPC boundary' });
        const args = call.arguments[1];
        const suppliedArgs = args ? checker.getTypeAtLocation(args) : undefined;
        for (const param of contract.params) {
            if (!nullable(param.type) && (!suppliedArgs || !checker.getPropertyOfType(suppliedArgs, param.name)))
                report.argumentNames.push({ command, location, missing: param.name });
        }
        if (args && ts.isObjectLiteralExpression(args)) {
            const expected = new Set(contract.params.map(p => p.name));
            for (const p of args.properties) {
                if (!p.name || ts.isComputedPropertyName(p.name))
                    continue;
                const name = p.name.getText(source).replaceAll("'", '');
                const param = contract.params.find(p => p.name === name);
                if (!param)
                    report.argumentNames.push({ command, location, unexpected: name, expected: [...expected] });
                else {
                    const value = ts.isPropertyAssignment(p) ? p.initializer : p.name;
                    const supplied = checker.getTypeAtLocation(value);
                    const checked = nullable(param.type) ? checker.getNonNullableType(supplied) : supplied;
                    // Object fields are checked separately to respect Serde's
                    // omitted Option fields, which Specta renders as nullable.
                    const wanted = checker.getNonNullableType(param.type);
                    if (!(wanted.flags & ts.TypeFlags.Object) && !checker.isTypeAssignableTo(checked, param.type))
                        report.argumentNames.push({ command, location, field: name, actual: checker.typeToString(supplied), expected: checker.typeToString(param.type) });
                    checkObject(value, param.type, { command, location }, name + '.');
                }
            }
        }
    });
}
if (process.argv.includes('--self-test')) {
    const isProbe = item => item.location.startsWith('src/__contract_probe__.ts:');
    const channelErrors = report.argumentNames.filter(item => isProbe(item) && item.command === 'generate_learning_program');
    if (report.mismatches.filter(isProbe).length !== 3 || report.uncovered.filter(isProbe).length !== 1
        || !report.mismatches.some(item => isProbe(item) && item.command === 'get_learning_runtime_catalog')
        || !report.mismatches.some(item => isProbe(item) && item.command === 'list_study_decks')
        || report.unrouted.filter(isProbe).length !== 1
        || !report.argumentNames.some(item => isProbe(item) && item.unexpected === 'request.documentId')
        || !report.argumentNames.some(item => isProbe(item) && item.missing === 'request.document_id')
        || !report.argumentNames.some(item => isProbe(item) && item.missing === 'deleteFile')
        || channelErrors.length !== 3 || channelErrors.some(item => item.field !== 'onProgress'))
        throw new Error('IPC guard failed to detect intentionally broken contracts');
    for (const key of ['mismatches', 'uncovered', 'unrouted', 'argumentNames']) report[key] = report[key].filter(item => !isProbe(item));
    report.checked -= 10;
    console.log('IPC guard rejects stale wrapped and direct responses, stale feature-wrapper responses, missing contracts, unrouted commands, incorrect nested request fields, and invalid transport channels.');
}
// Browser e2e fixtures answer IPC through e2e/fixtures/tauri.ts, typed by the
// same generated contracts; `tsc -p tsconfig.e2e.json` is what rejects a stale
// fixture. Prove the harness still makes it reject one.
if (process.argv.includes('--self-test')) {
    const e2eConfig = ts.readConfigFile(path.join(root, 'tsconfig.e2e.json'), ts.sys.readFile);
    const e2eParsed = ts.parseJsonConfigFileContent(e2eConfig.config, ts.sys, root);
    const e2eProbePath = path.join(root, 'e2e', '__contract_probe__.ts');
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
    const e2eHost = ts.createCompilerHost(e2eParsed.options);
    const getE2eSourceFile = e2eHost.getSourceFile.bind(e2eHost);
    e2eHost.getSourceFile = (file, ...args) => file === e2eProbePath
        ? ts.createSourceFile(file, e2eProbe, ts.ScriptTarget.Latest, true)
        : getE2eSourceFile(file, ...args);
    const e2eProgram = ts.createProgram([...e2eParsed.fileNames, e2eProbePath], e2eParsed.options, e2eHost);
    const rejected = ts.getPreEmitDiagnostics(e2eProgram, e2eProgram.getSourceFile(e2eProbePath))
        .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')).join('\n');
    for (const expected of ['"isArchivedd"', '"command_without_a_generated_contract"', '"modules.lessons.blocks.titl"', '"madeUpField"'])
        if (!rejected.includes(expected))
            throw new Error(`e2e IPC fixtures no longer reject ${expected}`);
    console.log('e2e IPC fixtures reject unknown commands and fields the generated DTOs lack.');
}
if (process.argv.includes('--json'))
    console.log(JSON.stringify(report, null, 2));
else {
    console.log(`Checked ${report.checked} IPC response boundaries against ${report.generatedCommands} generated command contracts.`);
    for (const item of [...report.mismatches, ...report.unrouted.map(item => ({ ...item, unrouted: true })), ...report.argumentNames])
        console.error(JSON.stringify(item));
    console.log(`${report.uncovered.length} call sites require explicit contracts (see --json).`);
}
if (report.mismatches.length || report.argumentNames.length || report.uncovered.length || report.unrouted.length)
    process.exitCode = 1;
