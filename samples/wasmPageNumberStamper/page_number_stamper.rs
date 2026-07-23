// ページ番号スタンパー（サンプルプラグイン、Rust実装）
//
// ホストとのやり取り（NUL終端UTF-8ポインタでの文字列マーシャリング、`host_system`
// モジュール名前空間からのホストAPIインポート、`alloc`エクスポート）はプラグインの
// 種別に依らない共通処理のため、`rust/host_sdk.rs`に切り出してある。
// このファイルはそれを`mod`宣言して使い、自身の業務ロジックだけを実装する
//
// ビルド方法（要 `rustup target add wasm32-unknown-unknown`）:
//   rustc --target wasm32-unknown-unknown -O page_number_stamper.rs -o page_number_stamper.wasm
#![crate_type = "cdylib"]

#[path = "../../rust/host_sdk.rs"]
mod host_sdk;
use host_sdk::*;

// このプラグインが自分で付与した注釈を再実行時に識別するための固定タグ
const STAMP_TAG: &str = "page-number-stamper";

// ============ 発見専用: describePlugin ============
// 実データの読み書きは一切行わない（実行時APIを誤って呼んではならない）

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn describePlugin() {
    register_entry_point(
        "stampPageNumbers",
        "ページ番号を配置",
        "各ページにページ番号を配置します（再実行時は前回分を置き換えます）",
    );

    // 処理対象文書。値はWASMへ渡らず、ホストが実行前にファイル選択ダイアログで解決する
    add_file_field("targetDoc", "対象文書", false);

    add_number_field("startPage", "開始ページ", 1.0, true);
    add_number_field("startNumber", "開始番号", 1.0, true);
    add_text_field("format", "表示フォーマット（{n}=番号 {total}=総数）", "{n}", true);
    add_select_field(
        "position",
        "配置位置",
        &[
            "bottom-center",
            "bottom-left",
            "bottom-right",
            "top-center",
            "top-left",
            "top-right",
        ],
        "bottom-center",
    );
    add_toggle_field("mirrorOddEven", "奇数偶数ページでミラー配置", false);
    add_number_field("fontSize", "フォントサイズ", 12.0, true);
    add_text_field("color", "文字色（#RRGGBB）", "#000000", true);
}

// ============ 実行時: エントリポイント本体 ============

/// 配置位置の奇偶ミラー（左右反転。center系はそのまま）
fn mirror_position(position: &str) -> String {
    match position {
        "bottom-left" => "bottom-right".to_string(),
        "bottom-right" => "bottom-left".to_string(),
        "top-left" => "top-right".to_string(),
        "top-right" => "top-left".to_string(),
        other => other.to_string(),
    }
}

fn compute_x(position: &str, page_width: f32, box_width: f32, margin: f32) -> f32 {
    if position.ends_with("left") {
        margin
    } else if position.ends_with("right") {
        page_width - box_width - margin
    } else {
        (page_width - box_width) / 2.0
    }
}

fn compute_y(position: &str, page_height: f32, box_height: f32, margin: f32) -> f32 {
    if position.starts_with("top") {
        margin
    } else {
        page_height - box_height - margin
    }
}

// 引数順: [システムコンテキスト] target_file_count, page_count, page_width, page_height →
//         [describePluginでの宣言順のうち、file型を除くもの] start_page, start_number,
//         format, position, mirror_odd_even, font_size, color
// （`targetDoc`はfile型フィールドのためWASMの引数には現れない。実行対象はホスト側で
// 解決済みで、doc.*/plan.*系ホストAPIが暗黙にその文書を操作対象とする。このプラグインは
// file型フィールドを1つしか宣言していないため、対象ファイルのfileIndexは常に0）
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn stampPageNumbers(
    _target_file_count: i32,
    page_count: i32,
    page_width: f32,
    page_height: f32,
    start_page: i32,
    start_number: i32,
    format_ptr: *const u8,
    position_ptr: *const u8,
    mirror_odd_even: bool,
    font_size: f32,
    color_ptr: *const u8,
) -> i32 {
    const FILE_INDEX: i32 = 0;
    let (format, position, color) =
        unsafe { (read_c_string(format_ptr), read_c_string(position_ptr), read_c_string(color_ptr)) };

    // 開始ページが総ページ数を超えている等の不正な入力は、実行前にエラーとして
    // 分かりやすく報告する（`ui.reportError`。呼ぶとラン全体が失敗扱いになる）
    if start_page < 1 || start_page > page_count {
        report_error(&format!(
            "開始ページ（{start_page}）が対象文書のページ数（{page_count}）の範囲外です"
        ));
        return 0;
    }

    // 全ページ一括の変更のため、確認は最初の1回のみでよいという判断
    set_confirmation_mode("once");

    // 1. 前回このプラグインが付与した注釈があれば、すべて削除予定として積む
    let previous_ids = get_annotation_ids_by_tag(FILE_INDEX, STAMP_TAG);
    for id in previous_ids.split(',') {
        if !id.is_empty() {
            remove_annotation(id);
        }
    }

    // 2. 新しい設定で全ページに付与し直す
    let box_width: f32 = 80.0;
    let box_height: f32 = font_size + 6.0;
    let margin: f32 = 20.0;
    let total = page_count - start_page + 1;
    let mut count = 0;

    let mut page = start_page;
    while page <= page_count {
        let is_even = page % 2 == 0;
        let effective_position = if mirror_odd_even && is_even {
            mirror_position(&position)
        } else {
            position.clone()
        };
        let x = compute_x(&effective_position, page_width, box_width, margin);
        let y = compute_y(&effective_position, page_height, box_height, margin);
        let n = start_number + (page - start_page);
        let text = format.replace("{n}", &n.to_string()).replace("{total}", &total.to_string());

        add_annotation(
            FILE_INDEX, page, x, y, box_width, box_height, &text, &color, font_size, STAMP_TAG,
        );

        report_progress(((page - start_page + 1) * 100) / total);
        log(&format!("{}/{total} ページ処理", page - start_page + 1));
        count += 1;
        page += 1;
    }

    count
}
