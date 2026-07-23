// 新規プラグイン開発用の空テンプレート
//
// このリポジトリ（RD-PluginSDK）をクローンし、このファイルに業務ロジックを実装していけば
// 新しいプラグインを開発できる。ホストとのやり取り（NUL終端UTF-8ポインタでの文字列
// マーシャリング、`host_system`モジュール名前空間からのホストAPIインポート、`alloc`
// エクスポート）はプラグインの種別に依らない共通処理のため`rust/host_sdk.rs`に切り出して
// あり、このファイルは`mod`宣言して使うだけでよい。実装例は`samples/`配下を参照
//
// ビルド方法（要 `rustup target add wasm32-unknown-unknown`）:
//   rustc --target wasm32-unknown-unknown -O main.rs -o main.wasm
#![crate_type = "cdylib"]

#[path = "rust/host_sdk.rs"]
mod host_sdk;
use host_sdk::*;

// ============ 発見専用: describePlugin ============
// ホストが最初に呼ぶ関数。エントリポイントと入力欄をここで宣言する。
// 実データの読み書きは一切行わない（実行時APIを誤って呼んではならない）

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn describePlugin() {
    register_entry_point(
        "entryPointId",
        "エントリポイント名",
        "エントリポイントの説明",
    );

    // 入力欄の宣言例（不要なものは削除してよい）
    // add_file_field("targetDoc", "対象文書", false);
    // add_text_field("someText", "テキスト入力", "", true);
    // add_number_field("someNumber", "数値入力", 0.0, true);
    // add_toggle_field("someToggle", "ON/OFF", false);
    // add_select_field("someSelect", "選択入力", &["optionA", "optionB"], "optionA");
}

// ============ 実行時: エントリポイント本体 ============
//
// 引数順: [システムコンテキスト] target_file_count, page_count, page_width, page_height →
//         [describePluginでの宣言順のうち、file型を除くもの]
// （file型フィールドはWASMの引数には現れない。対象ファイルはホスト側で解決済みで、
// doc.*/plan.*系ホストAPIの`file_index`引数がdescribePluginでのfile型フィールド宣言順を指す）

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn entryPointId(
    _target_file_count: i32,
    _page_count: i32,
    _page_width: f32,
    _page_height: f32,
) -> i32 {
    // ここに業務ロジックを実装する。
    // 例: report_progress(100) / log("...") / report_error("...") / add_annotation(...)
    0
}
