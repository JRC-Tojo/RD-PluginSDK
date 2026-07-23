/**
 * プラグイン開発者向けCLI: ローカルの.wasmファイルを、本体アプリと全く同じ
 * `src/services/plugin/engines/wasmEngine.ts`（discoverEntryPoints/runEntryPoint）に通し、
 * WASMのABI・`describePlugin`規約・エントリポイントの引数順序が壊れていないかを確認する
 *
 * 実行: `bun run plugin:test -- --wasm path/to/your_plugin.wasm [--entry entryId]
 *        [--field key=value ...] [--fixture path/to/fixture.json] [--manifest path/to/plugin.json]`
 *
 * 既知の限界: 実際のPDF内容・実DBの注釈は使わず、疑似コンテキスト（`--fixture`未指定時は
 * 1ページのダミー文書1つ分の既定値）に対して実行する。したがって業務ロジックの結果自体の
 * 正しさではなく、「WASMのABI・エントリポイント引数の順序」が壊れていないことの検証に
 * 主眼を置くツールである。実文書に対する動作確認は、アプリ内の「WASMを直接インストール」
 * 機能（サイドロード）から行うこと
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { discoverEntryPoints, runEntryPoint } from 'src/services/plugin/engines/wasmEngine';
import { buildPositionalArgs } from 'src/services/plugin/positionalArgs';
import type { ExecutionState } from 'src/services/plugin/hostApiBridge';
import type { PluginExecutionContext, PluginFileContext } from 'src/services/plugin/hostContext';
import type { PluginManifest } from 'src/models/plugin/manifest';
import { PluginHostApiName as PluginHostApiNameSchema } from 'src/models/plugin/manifest';
import type { ContainerElementFile, ContainerID } from 'src/models/container';

export interface CliArgs {
  wasmPath: string;
  entryId: string | undefined;
  fieldValues: Record<string, string | number | boolean>;
  fixturePath: string | undefined;
  manifestPath: string | undefined;
}

export function parseArgs(argv: string[]): CliArgs {
  const fieldValues: Record<string, string | number | boolean> = {};
  let wasmPath: string | undefined;
  let entryId: string | undefined;
  let fixturePath: string | undefined;
  let manifestPath: string | undefined;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--wasm') wasmPath = argv[++i];
    else if (arg === '--entry') entryId = argv[++i];
    else if (arg === '--fixture') fixturePath = argv[++i];
    else if (arg === '--manifest') manifestPath = argv[++i];
    else if (arg === '--field') {
      const pair = argv[++i] ?? '';
      const eqIndex = pair.indexOf('=');
      if (eqIndex === -1) continue;
      const key = pair.slice(0, eqIndex);
      const rawValue = pair.slice(eqIndex + 1);
      const numeric = Number(rawValue);
      if (rawValue === 'true') fieldValues[key] = true;
      else if (rawValue === 'false') fieldValues[key] = false;
      else if (rawValue !== '' && !Number.isNaN(numeric)) fieldValues[key] = numeric;
      else fieldValues[key] = rawValue;
    }
  }

  if (!wasmPath) {
    throw new Error('--wasm <path> は必須です');
  }
  return { wasmPath, entryId, fieldValues, fixturePath, manifestPath };
}

/** `--fixture`のJSON構造（Mapの代わりにプレーンなRecordで持つ。JSON往復のため） */
interface FileContextFixture {
  pageCount?: number;
  metadataJson?: string;
  pageSizes?: Record<string, { width: number; height: number }>;
  pageTextBlocksJson?: Record<string, string>;
  pageImages?: Record<string, string>;
}
interface ContextFixture {
  targetFileCount?: number;
  fileContexts?: FileContextFixture[];
  representativePageSize?: { width: number; height: number };
}

const DUMMY_CONTAINER_ID = '11111111-1111-4111-8111-111111111111' as ContainerID;

function buildDummyFile(index: number): ContainerElementFile {
  return {
    containerID: DUMMY_CONTAINER_ID,
    type: 'File',
    path: `dummy-${index}.pdf`,
    createdAt: new Date(),
    updatedAt: new Date(),
    description: '',
    genre: '',
    tags: [],
  };
}

function toFileContext(fixture: FileContextFixture | undefined): PluginFileContext {
  return {
    pageCount: fixture?.pageCount ?? 1,
    metadataJson: fixture?.metadataJson ?? '{}',
    pageSizes: new Map(
      Object.entries(fixture?.pageSizes ?? { '1': { width: 595, height: 842 } }).map(([k, v]) => [
        Number(k),
        v,
      ]),
    ),
    pageTextBlocksJson: new Map(
      Object.entries(fixture?.pageTextBlocksJson ?? {}).map(([k, v]) => [Number(k), v]),
    ),
    pageImages: new Map(Object.entries(fixture?.pageImages ?? {}).map(([k, v]) => [Number(k), v])),
    existingAnnotations: [],
  };
}

export function buildContext(fixture: ContextFixture | undefined): PluginExecutionContext {
  const fileCount = fixture?.targetFileCount ?? fixture?.fileContexts?.length ?? 1;
  const targetFiles = Array.from({ length: fileCount }, (_, i) => buildDummyFile(i));
  const fileContexts =
    fixture?.fileContexts && fixture.fileContexts.length > 0
      ? fixture.fileContexts.map(toFileContext)
      : targetFiles.map(() => toFileContext(undefined));

  return {
    targetFiles,
    fileContexts,
    representativePageSize: fixture?.representativePageSize ?? { width: 595, height: 842 },
  };
}

function loadManifest(manifestPath: string | undefined): PluginManifest {
  if (!manifestPath) {
    // ローカル動作確認用ツールのため、全ホストAPIを許可した最大権限マニフェストを既定とする
    return {
      id: 'dev-check' as never,
      name: 'dev-check',
      version: '0.0.0',
      description: '',
      runtime: 'wasm',
      mainFile: 'dev-check.wasm',
      requiredHostApis: PluginHostApiNameSchema.options,
    };
  }
  const json = JSON.parse(readFileSync(manifestPath, 'utf-8')) as unknown;
  return json as PluginManifest;
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  const binary = new Uint8Array(readFileSync(path.resolve(args.wasmPath)));

  console.log(`--- describePlugin() を発見専用モードで実行 ---`);
  const discoverRes = await discoverEntryPoints(binary);
  if (!discoverRes.ok) {
    console.error(`発見に失敗しました: ${discoverRes.error.message}`);
    process.exitCode = 1;
    return;
  }
  for (const descriptor of discoverRes.value) {
    console.log(`\nentryId: ${descriptor.entryId}  (${descriptor.label})`);
    console.log(`  ${descriptor.description}`);
    for (const field of descriptor.fields) {
      console.log(`  - ${field.fieldId}: ${field.type}${field.optional ? ' (任意)' : ' (必須)'}`);
    }
  }

  if (!args.entryId) {
    console.log('\n--entry が指定されていないため、発見結果の表示のみで終了します');
    return;
  }

  const descriptor = discoverRes.value.find((d) => d.entryId === args.entryId);
  if (!descriptor) {
    console.error(`entryId "${args.entryId}" は発見されませんでした`);
    process.exitCode = 1;
    return;
  }

  const manifest = loadManifest(args.manifestPath);
  const fixture = args.fixturePath
    ? (JSON.parse(readFileSync(args.fixturePath, 'utf-8')) as ContextFixture)
    : undefined;
  const ctx = buildContext(fixture);
  const positionalArgs = buildPositionalArgs(descriptor, args.fieldValues, ctx);
  const state: ExecutionState = { blocks: [], plan: [], confirmationMode: 'perItem' };

  console.log(`\n--- ${args.entryId}(${positionalArgs.join(', ')}) を実行 ---`);
  const runRes = await runEntryPoint(binary, args.entryId, positionalArgs, manifest, ctx, state);
  if (!runRes.ok) {
    console.error(`実行エラー: ${runRes.error.message}`);
    process.exitCode = 1;
    return;
  }

  console.log(`戻り値: ${JSON.stringify(runRes.value)}`);
  console.log(`\nblocks (進捗/ログ):`);
  console.log(JSON.stringify(state.blocks, null, 2));
  console.log(`\nplan (書き込み予定項目、${state.plan.length}件):`);
  console.log(JSON.stringify(state.plan, null, 2));
}

if (import.meta.main) {
  await main();
}
