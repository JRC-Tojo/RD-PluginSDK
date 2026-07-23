// RelationalDocuments WASMプラグイン向けRust SDK
//
// ホスト（RelationalDocumentsアプリ）とのやり取りに必要な、プラグインの種別に依らず
// 共通の実装となる部分（`host_system`モジュール名前空間からのホストAPIインポート宣言、
// NUL終端UTF-8ポインタ⇔Rust文字列の変換、ホストが文字列を書き込むための`alloc`
// エクスポート）をまとめたモジュール。個々のプラグイン本体（例:
// `samples/wasmPageNumberStamper/page_number_stamper.rs`や、新規開発時は`main.rs`）は、
// このファイルを相対パスで`mod`宣言して`use`し、ここで定義する安全なラッパー関数だけを
// 呼び出せばよい:
//
//   #[path = "../../rust/host_sdk.rs"]
//   mod host_sdk;
//   use host_sdk::*;
//
// 呼び出し規約の詳細はRD-PluginStockリポジトリの`docs/PLUGIN_DEVELOPMENT_GUIDE.md`
// を参照（文字列はすべてNUL終端UTF-8バイト列へのポインタとしてやり取りする）

use std::alloc::{alloc as std_alloc, Layout};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// ============ ホストAPI（`host_system`モジュール名前空間からインポート） ============
// 発見専用API（常時付与）と実行時API（plugin.jsonのrequiredHostApisで許可されたもののみ、
// 呼び出し時にホスト側がダミー実装へ差し替える）。プラグインが実際に使わないAPIも
// ここでは宣言だけしておいて構わない（呼ばれなければ問題にならない）
// 以下extern "C"ブロックの中身は `bun run generate:plugin-sdk` により
// `src/services/plugin/hostApiRegistry.ts` から自動生成される。手で編集しないこと
// （`src/services/plugin/__test__/hostApiCodegen.test.ts` がズレを検知する）
#[link(wasm_import_module = "host_system")]
extern "C" {
    // GENERATED-EXTERN:BEGIN
    // ---- 発見専用（describePlugin内でのみ呼ぶこと） ----
    fn ui_register_entry_point(entry_id: *const u8, label: *const u8, description: *const u8);
    fn ui_add_text_field(
        field_id: *const u8,
        label: *const u8,
        default_value: *const u8,
        optional: bool,
    );
    fn ui_add_number_field(
        field_id: *const u8,
        label: *const u8,
        default_value: f64,
        optional: bool,
    );
    fn ui_add_toggle_field(field_id: *const u8, label: *const u8, default_value: bool);
    fn ui_add_select_field(
        field_id: *const u8,
        label: *const u8,
        options_csv: *const u8,
        default_value: *const u8,
    );
    fn ui_add_file_field(field_id: *const u8, label: *const u8, optional: bool);

    // ---- 実行時API（manifest.requiredHostApisで要求したもののみ実データを返す） ----
    fn ui_report_progress(percent: i32);
    fn ui_log(message: *const u8);
    fn ui_report_error(message: *const u8);
    fn plan_set_confirmation_mode(mode: *const u8);
    fn plan_add_annotation(
        file_index: i32,
        page: i32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text: *const u8,
        color: *const u8,
        font_size: f32,
        tags_csv: *const u8,
    ) -> *const u8;
    fn plan_update_annotation(
        annot_id: *const u8,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text: *const u8,
        color: *const u8,
        font_size: f32,
        tags_csv: *const u8,
    ) -> *const u8;
    fn plan_remove_annotation(annot_id: *const u8) -> *const u8;
    fn plan_add_relational(
        src_annot_id: *const u8,
        target_annot_id: *const u8,
        rule_type: *const u8,
    ) -> *const u8;
    fn plan_remove_relational(src_annot_id: *const u8, target_annot_id: *const u8) -> *const u8;
    fn doc_get_project_metadata(file_index: i32) -> *const u8;
    fn doc_get_page_size(file_index: i32, page: i32) -> *const u8;
    fn doc_get_page_text_blocks(file_index: i32, page: i32) -> *const u8;
    fn doc_get_page_image(file_index: i32, page: i32) -> *const u8;
    fn doc_get_annotations_by_file(file_index: i32) -> *const u8;
    fn doc_get_annotation_ids_by_tag(file_index: i32, tag: *const u8) -> *const u8;
    // GENERATED-EXTERN:END
}

// ============ ホストが文字列を書き込むための領域確保（規約上必須のエクスポート） ============

/// ホストがこの関数を呼び、返ってきたポインタへNUL終端UTF-8バイト列を書き込む。
/// 1回の実行内で使い捨てる領域のため解放は行わない（毎回新しいWASMインスタンスで実行される）
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    if size == 0 {
        return std::ptr::null_mut();
    }
    unsafe {
        let layout = Layout::from_size_align(size, 1).expect("invalid layout size");
        std_alloc(layout)
    }
}

// ============ 文字列マーシャリングのヘルパー ============

/// ptrが指すNUL終端UTF-8バイト列を読み取り、Rustの`String`に変換する
///
/// # Safety
/// `ptr`はホストが書き込んだ有効なNUL終端UTF-8バイト列を指しているか、nullである必要がある
pub unsafe fn read_c_string(ptr: *const u8) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let cstr = CStr::from_ptr(ptr as *const c_char);
    cstr.to_string_lossy().into_owned()
}

/// Rustの文字列をNUL終端UTF-8の`CString`に変換する（ホスト関数呼び出しの引数用）
pub fn to_c_string(s: &str) -> CString {
    // 文字列内にNULバイトが含まれることは通常ないが、含まれていた場合は安全側に倒し
    // そこで切り詰める（プラグイン側の実装ミスでホスト呼び出し自体が失敗しないようにする）
    CString::new(s).unwrap_or_else(|e| {
        let valid_up_to = e.nul_position();
        CString::new(&e.into_vec()[..valid_up_to]).unwrap()
    })
}

// ============ 発見専用API：安全なラッパー ============
// `describePlugin()`からのみ呼ぶこと（実行時APIと違いrequiredHostApisの宣言は不要）

/// エントリポイントを1件登録する。以降の`add_*_field`呼び出しはこのエントリポイントに紐づく
pub fn register_entry_point(entry_id: &str, label: &str, description: &str) {
    let e = to_c_string(entry_id);
    let l = to_c_string(label);
    let d = to_c_string(description);
    unsafe {
        ui_register_entry_point(
            e.as_ptr() as *const u8,
            l.as_ptr() as *const u8,
            d.as_ptr() as *const u8,
        );
    }
}

/// 文字列入力欄を追加する
pub fn add_text_field(field_id: &str, label: &str, default_value: &str, optional: bool) {
    let f = to_c_string(field_id);
    let l = to_c_string(label);
    let d = to_c_string(default_value);
    unsafe {
        ui_add_text_field(
            f.as_ptr() as *const u8,
            l.as_ptr() as *const u8,
            d.as_ptr() as *const u8,
            optional,
        );
    }
}

/// 数値入力欄を追加する
pub fn add_number_field(field_id: &str, label: &str, default_value: f64, optional: bool) {
    let f = to_c_string(field_id);
    let l = to_c_string(label);
    unsafe {
        ui_add_number_field(f.as_ptr() as *const u8, l.as_ptr() as *const u8, default_value, optional);
    }
}

/// ON/OFFスイッチを追加する
pub fn add_toggle_field(field_id: &str, label: &str, default_value: bool) {
    let f = to_c_string(field_id);
    let l = to_c_string(label);
    unsafe {
        ui_add_toggle_field(f.as_ptr() as *const u8, l.as_ptr() as *const u8, default_value);
    }
}

/// 選択式入力欄を追加する（`options`はUIの選択肢一覧。内部でカンマ区切りに変換して渡す）
pub fn add_select_field(field_id: &str, label: &str, options: &[&str], default_value: &str) {
    let f = to_c_string(field_id);
    let l = to_c_string(label);
    let o = to_c_string(&options.join(","));
    let d = to_c_string(default_value);
    unsafe {
        ui_add_select_field(
            f.as_ptr() as *const u8,
            l.as_ptr() as *const u8,
            o.as_ptr() as *const u8,
            d.as_ptr() as *const u8,
        );
    }
}

/// 処理対象文書を1件選択させる入力欄を追加する（値そのものはWASMへ渡らず、ホストが
/// 実行前にファイル選択ダイアログで解決する。詳細は開発者ガイド参照）
pub fn add_file_field(field_id: &str, label: &str, optional: bool) {
    let f = to_c_string(field_id);
    let l = to_c_string(label);
    unsafe {
        ui_add_file_field(f.as_ptr() as *const u8, l.as_ptr() as *const u8, optional);
    }
}

// ============ 実行時API：安全なラッパー ============
// 呼び出すには対応するAPI名をplugin.jsonのrequiredHostApisに宣言しておくこと

/// 実行進捗（0〜100）を報告する
pub fn report_progress(percent: i32) {
    unsafe { ui_report_progress(percent) };
}

/// ログを1行出力する（プラグインタブに蓄積表示される。複数回呼ぶと行が積み上がる）
pub fn log(message: &str) {
    let m = to_c_string(message);
    unsafe { ui_log(m.as_ptr() as *const u8) };
}

/// プラグイン自身が判断した実行失敗を報告する。WASM呼び出し自体は正常に戻っても、
/// これを呼ぶとホスト側はラン全体を失敗（エラー）扱いにする
pub fn report_error(message: &str) {
    let m = to_c_string(message);
    unsafe { ui_report_error(m.as_ptr() as *const u8) };
}

/// 以降の`plan.*`呼び出しに適用する確認モードを設定する（`"once"`または`"perItem"`）
pub fn set_confirmation_mode(mode: &str) {
    let m = to_c_string(mode);
    unsafe { plan_set_confirmation_mode(m.as_ptr() as *const u8) };
}

/// アノテーションの新規作成予定を積む（`file_index`は`describePlugin`での`file`型
/// フィールド宣言順。0が最初に宣言したファイル）。戻り値は積まれた予定項目のID
#[allow(clippy::too_many_arguments)]
pub fn add_annotation(
    file_index: i32,
    page: i32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text: &str,
    color: &str,
    font_size: f32,
    tags_csv: &str,
) -> String {
    let text_c = to_c_string(text);
    let color_c = to_c_string(color);
    let tags_c = to_c_string(tags_csv);
    unsafe {
        let ptr = plan_add_annotation(
            file_index,
            page,
            x,
            y,
            width,
            height,
            text_c.as_ptr() as *const u8,
            color_c.as_ptr() as *const u8,
            font_size,
            tags_c.as_ptr() as *const u8,
        );
        read_c_string(ptr)
    }
}

/// 既存アノテーションの変更予定を積む。戻り値は積まれた予定項目のID
#[allow(clippy::too_many_arguments)]
pub fn update_annotation(
    annot_id: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text: &str,
    color: &str,
    font_size: f32,
    tags_csv: &str,
) -> String {
    let id_c = to_c_string(annot_id);
    let text_c = to_c_string(text);
    let color_c = to_c_string(color);
    let tags_c = to_c_string(tags_csv);
    unsafe {
        let ptr = plan_update_annotation(
            id_c.as_ptr() as *const u8,
            x,
            y,
            width,
            height,
            text_c.as_ptr() as *const u8,
            color_c.as_ptr() as *const u8,
            font_size,
            tags_c.as_ptr() as *const u8,
        );
        read_c_string(ptr)
    }
}

/// 既存アノテーションの削除予定を積む。戻り値は積まれた予定項目のID
pub fn remove_annotation(annot_id: &str) -> String {
    let id_c = to_c_string(annot_id);
    unsafe {
        let ptr = plan_remove_annotation(id_c.as_ptr() as *const u8);
        read_c_string(ptr)
    }
}

/// 関係性の新規作成予定を積む（`rule_type`は`"link"`または`"equal"`）。戻り値は積まれた予定項目のID
pub fn add_relational(src_annot_id: &str, target_annot_id: &str, rule_type: &str) -> String {
    let src_c = to_c_string(src_annot_id);
    let target_c = to_c_string(target_annot_id);
    let rule_c = to_c_string(rule_type);
    unsafe {
        let ptr = plan_add_relational(
            src_c.as_ptr() as *const u8,
            target_c.as_ptr() as *const u8,
            rule_c.as_ptr() as *const u8,
        );
        read_c_string(ptr)
    }
}

/// 既存関係性1本の削除予定を積む。戻り値は積まれた予定項目のID
pub fn remove_relational(src_annot_id: &str, target_annot_id: &str) -> String {
    let src_c = to_c_string(src_annot_id);
    let target_c = to_c_string(target_annot_id);
    unsafe {
        let ptr = plan_remove_relational(src_c.as_ptr() as *const u8, target_c.as_ptr() as *const u8);
        read_c_string(ptr)
    }
}

/// 指定ファイル（`file_index`は`describePlugin`での`file`型フィールド宣言順）の
/// プロジェクトメタ情報（JSON文字列）を取得する
pub fn get_project_metadata(file_index: i32) -> String {
    unsafe { read_c_string(doc_get_project_metadata(file_index)) }
}

/// 指定ファイル・ページのサイズ（JSON文字列 `{width, height}`）を取得する
pub fn get_page_size(file_index: i32, page: i32) -> String {
    unsafe { read_c_string(doc_get_page_size(file_index, page)) }
}

/// 指定ファイル・ページの位置情報付きテキスト（JSON配列文字列）を取得する
pub fn get_page_text_blocks(file_index: i32, page: i32) -> String {
    unsafe { read_c_string(doc_get_page_text_blocks(file_index, page)) }
}

/// 指定ファイル・ページのレンダリング画像（base64 PNG文字列）を取得する
pub fn get_page_image(file_index: i32, page: i32) -> String {
    unsafe { read_c_string(doc_get_page_image(file_index, page)) }
}

/// 指定ファイルの既存アノテーション一覧（JSON配列文字列）を取得する
pub fn get_annotations_by_file(file_index: i32) -> String {
    unsafe { read_c_string(doc_get_annotations_by_file(file_index)) }
}

/// 指定ファイルのうち、指定タグを持つ既存アノテーションIDのみをCSVで取得する（軽量版）
pub fn get_annotation_ids_by_tag(file_index: i32, tag: &str) -> String {
    let tag_c = to_c_string(tag);
    unsafe { read_c_string(doc_get_annotation_ids_by_tag(file_index, tag_c.as_ptr() as *const u8)) }
}
