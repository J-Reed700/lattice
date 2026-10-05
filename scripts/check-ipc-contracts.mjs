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
            contracts.set(call.arguments[0].text, { type: checker.getTypeFromTypeNode(response), text: response.getText(bindings), params: node.parameters.map(p => ({ name: p.name.getText(bindings), type: checker.getTypeAtLocation(p) })) });
    });
});
const api = program.getSourceFile(path.join(root, 'src/shared/ipc/routes.ts')), routes = new Map();
visit(api, node => {
    if (!ts.isVariableDeclaration(node) || node.name.getText(api) !== 'COMMAND_DOMAIN_MAP')
        return;
    for (const p of node.initializer.properties)
        routes.set(p.name.getText(api).replaceAll("'", ''), p.initializer.properties.find(q => q.name.getText(api) === 'command').initializer.text);
});
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
        if (!ts.isCallExpression(call) || !['apiCall', 'invoke'].includes(call.expression.getText(source)) || !ts.isStringLiteral(call.arguments[0]))
            return;
        const inputName = call.arguments[0].text, command = (routes.get(inputName) ?? inputName).replace(/^plugin:[^|]+\|/, '');
        const location = `${path.relative(root, source.fileName)}:${source.getLineAndCharacterOfPosition(call.getStart(source)).line + 1}`;
        // `apiCall` throws at runtime on a command COMMAND_DOMAIN_MAP does not
        // name, so an unrouted call site is a dead feature, not a fallback.
        if (call.expression.getText(source) === 'apiCall' && !routes.has(inputName)) {
            report.unrouted.push({ command: inputName, location });
            return;
        }
        const contract = contracts.get(command);
        let declared = call.typeArguments?.[0] ? checker.getTypeFromTypeNode(call.typeArguments[0]) : checker.getAwaitedType(checker.getTypeAtLocation(call));
        if (!call.typeArguments?.[0] && call.expression.getText(source) === 'apiCall') {
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
    if (report.mismatches.filter(isProbe).length !== 2 || report.uncovered.filter(isProbe).length !== 1
        || !report.mismatches.some(item => isProbe(item) && item.command === 'get_learning_runtime_catalog')
        || report.unrouted.filter(isProbe).length !== 1
        || !report.argumentNames.some(item => isProbe(item) && item.unexpected === 'request.documentId')
        || !report.argumentNames.some(item => isProbe(item) && item.missing === 'request.document_id')
        || !report.argumentNames.some(item => isProbe(item) && item.missing === 'deleteFile')
        || channelErrors.length !== 3 || channelErrors.some(item => item.field !== 'onProgress'))
        throw new Error('IPC guard failed to detect intentionally broken contracts');
    for (const key of ['mismatches', 'uncovered', 'unrouted', 'argumentNames']) report[key] = report[key].filter(item => !isProbe(item));
    report.checked -= 9;
    console.log('IPC guard rejects stale wrapped and direct responses, missing contracts, unrouted commands, incorrect nested request fields, and invalid transport channels.');
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
