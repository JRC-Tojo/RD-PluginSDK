/**
 * `runPlugin.ts`（プラグイン開発者向けCLI）のうち、ファイルI/O・プロセス終了を伴わない
 * 純粋なロジック（引数解析・疑似コンテキスト組み立て）と、実際のWASMランタイム連携
 * （discoverEntryPoints/runEntryPointの失敗ハンドリング）を検証する
 */
import { describe, expect, test } from 'bun:test';
import { discoverEntryPoints, runEntryPoint } from 'src/services/plugin/engines/wasmEngine';
import { buildContext, parseArgs } from '../runPlugin';

// wasmEngine.test.tsと同じ、手書きの最小WASM固定値（alloc(size)->ptr のみをエクスポートし、
// describePluginは持たない）。この環境にRust/AssemblyScript等のツールチェインが無くても
// テストできるようにするため
// prettier-ignore
const MINIMAL_ALLOC_MODULE = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
  0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01, 0x7f,
  0x03, 0x02, 0x01, 0x00,
  0x05, 0x03, 0x01, 0x00, 0x02,
  0x06, 0x07, 0x01, 0x7f, 0x01, 0x41, 0x80, 0x08, 0x0b,
  0x07, 0x12, 0x02,
  0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00,
  0x05, 0x61, 0x6c, 0x6c, 0x6f, 0x63, 0x00, 0x00,
  0x0a, 0x13, 0x01, 0x11, 0x01, 0x01, 0x7f,
  0x23, 0x00,
  0x21, 0x01,
  0x23, 0x00,
  0x20, 0x00,
  0x6a,
  0x24, 0x00,
  0x20, 0x01,
  0x0b,
]);

describe('parseArgs', () => {
  test('--wasm必須、--field key=valueは型を推測して数値/真偽値/文字列に変換する', () => {
    const args = parseArgs([
      '--wasm',
      'plugin.wasm',
      '--entry',
      'doSomething',
      '--field',
      'count=5',
      '--field',
      'flag=true',
      '--field',
      'label=hello',
    ]);

    expect(args.wasmPath).toBe('plugin.wasm');
    expect(args.entryId).toBe('doSomething');
    expect(args.fieldValues).toEqual({ count: 5, flag: true, label: 'hello' });
  });

  test('--wasmが無い場合はエラーを投げる', () => {
    expect(() => parseArgs(['--entry', 'x'])).toThrow();
  });
});

describe('buildContext', () => {
  test('fixture未指定時は1ページのダミー文書1つ分の既定コンテキストを組み立てる', () => {
    const ctx = buildContext(undefined);
    expect(ctx.targetFiles).toHaveLength(1);
    expect(ctx.fileContexts).toHaveLength(1);
    expect(ctx.fileContexts[0]?.pageCount).toBe(1);
    expect(ctx.representativePageSize).toEqual({ width: 595, height: 842 });
  });

  test('fixtureで複数fileContextsを指定した場合、その件数ぶんのtargetFilesが組み立てられる', () => {
    const ctx = buildContext({
      fileContexts: [{ pageCount: 3 }, { pageCount: 5 }],
    });
    expect(ctx.targetFiles).toHaveLength(2);
    expect(ctx.fileContexts.map((fc) => fc.pageCount)).toEqual([3, 5]);
  });
});

describe('discoverEntryPoints/runEntryPointとの連携（実際のWASMランタイム経由）', () => {
  test('describePluginを持たないWASMに対する発見はFailureを返す', async () => {
    const res = await discoverEntryPoints(MINIMAL_ALLOC_MODULE);
    expect(res.ok).toBe(false);
  });

  test('存在しないエントリポイントの実行はFailureを返す', async () => {
    const ctx = buildContext(undefined);
    const manifest = {
      id: 'dev-check' as never,
      name: 'dev-check',
      version: '0.0.0',
      description: '',
      runtime: 'wasm' as const,
      mainFile: 'dev-check.wasm',
      requiredHostApis: [],
    };
    const state = { blocks: [], plan: [], confirmationMode: 'perItem' as const };
    const res = await runEntryPoint(MINIMAL_ALLOC_MODULE, 'doesNotExist', [], manifest, ctx, state);
    expect(res.ok).toBe(false);
  });
});
