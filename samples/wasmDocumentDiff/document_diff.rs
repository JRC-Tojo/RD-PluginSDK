// 文書差分検出（サンプルプラグイン、Rust実装）
//
// 複数の`file`型フィールド（`oldDoc`/`newDoc`）を使い、2つの文書を明示的に選択させて
// 横断的に読み書きする例。`doc.*`/`plan.addAnnotation`系ホストAPIの第1引数`fileIndex`は
// `describePlugin`での`file`型フィールドの宣言順（0始まり）に対応する。このプラグインでは
// oldDoc=0、newDoc=1として、`get_page_text_blocks(0, page)`/`get_page_text_blocks(1, page)`
// のように読み分け、`add_annotation(1, ...)`で新版側にのみ差分マークを付与する
//
// ホストとのやり取り（NUL終端UTF-8ポインタでの文字列マーシャリング、`host_system`
// モジュール名前空間からのホストAPIインポート、`alloc`エクスポート）は`rust/host_sdk.rs`
// に切り出してあり、このファイルは業務ロジックだけを実装する
//
// ビルド方法（要 `rustup target add wasm32-unknown-unknown`）:
//   rustc --target wasm32-unknown-unknown -O document_diff.rs -o document_diff.wasm
#![crate_type = "cdylib"]

#[path = "../../rust/host_sdk.rs"]
mod host_sdk;
use host_sdk::*;

const OLD_INDEX: i32 = 0;
const NEW_INDEX: i32 = 1;
// このプラグインが自分で付与した差分マークを再実行時に識別するための固定タグ
const DIFF_TAG: &str = "document-diff";

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn describePlugin() {
    register_entry_point(
        "compareDocuments",
        "新旧文書を比較",
        "旧版・新版の各ページのテキストを比較し、差異があるページの新版側に印を付けます（再実行時は前回分を置き換えます）",
    );

    // 複数のfile型フィールド。宣言順がそのままfileIndex（0=oldDoc, 1=newDoc）になる
    add_file_field("oldDoc", "旧版文書", false);
    add_file_field("newDoc", "新版文書", false);

    add_text_field("color", "印の色（#RRGGBB）", "#ff8800", true);
    add_number_field("fontSize", "フォントサイズ", 12.0, true);
}

/// `doc.getPageTextBlocks`が返すJSON配列文字列（`[{text, x, y, width, height}, ...]`）から、
/// 各要素の"text"フィールドの値だけを連結して取り出す簡易パーサ
///
/// 本格的なJSONパーサではなく、ホストが生成する決まった形式の文字列だけを前提とした
/// 簡易実装（エスケープされた引用符には非対応）。外部クレートを使わずビルドする方針
/// （§8参照）のため、この程度の単純な比較用途であれば十分と判断した
fn extract_concatenated_text(json_array: &str) -> String {
    let mut result = String::new();
    let key = "\"text\":\"";
    let mut rest = json_array;
    while let Some(start) = rest.find(key) {
        rest = &rest[start + key.len()..];
        match rest.find('"') {
            Some(end) => {
                result.push_str(&rest[..end]);
                rest = &rest[end + 1..];
            }
            None => break,
        }
    }
    result
}

// 引数順: [システムコンテキスト] target_file_count, page_count, page_width, page_height →
//         [describePluginでの宣言順のうち、file型を除くもの] color, font_size
// page_countはtargetFiles[0]（oldDoc）のページ数。newDoc側のページはoldDocと同じ
// ページ番号までを比較対象とする（newDocの方がページ数が多い/少ない場合の端数は比較しない）
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn compareDocuments(
    target_file_count: i32,
    page_count: i32,
    page_width: f32,
    _page_height: f32,
    color_ptr: *const u8,
    font_size: f32,
) -> i32 {
    // oldDoc/newDocのいずれかが未選択の場合、無言でreturnせず`ui.reportError`で
    // 分かりやすく報告する（両フィールドとも必須にしているため通常は発生しないが、
    // 念のための防御的チェック）
    if target_file_count < 2 {
        report_error("旧版・新版の両方の文書を選択してください");
        return 0;
    }

    let color = unsafe { read_c_string(color_ptr) };

    // 全ページ一括の変更のため、確認は最初の1回のみでよいという判断
    set_confirmation_mode("once");

    // 1. 前回このプラグインが新版文書へ付与した差分マークがあれば、すべて削除予定として積む
    let previous_ids = get_annotation_ids_by_tag(NEW_INDEX, DIFF_TAG);
    for id in previous_ids.split(',') {
        if !id.is_empty() {
            remove_annotation(id);
        }
    }

    // 2. 各ページのテキストを比較し、異なるページの新版側に印を付ける
    let box_width: f32 = 160.0;
    let box_height: f32 = font_size + 6.0;
    let margin: f32 = 20.0;
    let mut diff_count = 0;

    let mut page = 1;
    while page <= page_count {
        let old_text = extract_concatenated_text(&get_page_text_blocks(OLD_INDEX, page));
        let new_text = extract_concatenated_text(&get_page_text_blocks(NEW_INDEX, page));

        if old_text != new_text {
            let x = page_width - box_width - margin;
            let y = margin;
            add_annotation(
                NEW_INDEX, page, x, y, box_width, box_height, "差分あり", &color, font_size, DIFF_TAG,
            );
            log(&format!("p.{page}: 差分あり"));
            diff_count += 1;
        }

        report_progress((page * 100) / page_count);
        page += 1;
    }

    diff_count
}
