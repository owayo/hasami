//! C ABI (FFI) インターフェース
//!
//! C/C++ や他の言語からhasamiを利用するためのインターフェース

use crate::analyzer::Analyzer;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

/// FFI用のトークン
#[repr(C)]
pub struct HasamiToken {
    /// 表層形 (UTF-8, null-terminated)
    pub surface: *mut c_char,
    /// 開始バイト位置
    pub start: u32,
    /// 終了バイト位置
    pub end: u32,
    /// 品詞情報 (UTF-8, null-terminated)
    pub pos: *mut c_char,
    /// 原形 (UTF-8, null-terminated)
    pub base_form: *mut c_char,
    /// 読み (UTF-8, null-terminated)
    pub reading: *mut c_char,
    /// 辞書由来フラグ
    pub is_known: u8,
}

/// FFI用のトークンリスト
#[repr(C)]
pub struct HasamiTokenList {
    pub tokens: *mut HasamiToken,
    pub len: u32,
    pub capacity: u32,
}

/// アナライザーハンドル
pub struct HasamiAnalyzer {
    inner: Analyzer,
    last_error: Option<CString>,
}

/// 辞書ファイルからアナライザーを生成
///
/// # Safety
/// dict_path は有効なUTF-8 null終端文字列へのポインタ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hasami_new(dict_path: *const c_char) -> *mut HasamiAnalyzer {
    unsafe {
        if dict_path.is_null() {
            return ptr::null_mut();
        }

        let path = match CStr::from_ptr(dict_path).to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        match Analyzer::load(path) {
            Ok(analyzer) => Box::into_raw(Box::new(HasamiAnalyzer {
                inner: analyzer,
                last_error: None,
            })),
            Err(e) => {
                eprintln!("hasami_new error: {}", e);
                ptr::null_mut()
            }
        }
    }
}

/// テキストを形態素解析
///
/// # Safety
/// handle と text は有効なポインタ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hasami_tokenize(
    handle: *mut HasamiAnalyzer,
    text: *const c_char,
) -> HasamiTokenList {
    unsafe {
        let empty = HasamiTokenList {
            tokens: ptr::null_mut(),
            len: 0,
            capacity: 0,
        };

        if handle.is_null() || text.is_null() {
            return empty;
        }

        let analyzer = &mut (*handle).inner;
        let input = match CStr::from_ptr(text).to_str() {
            Ok(s) => s,
            Err(_) => return empty,
        };

        let tokens = analyzer.tokenize(input);
        let len = tokens.len();

        if len == 0 {
            return empty;
        }

        let mut ffi_tokens: Vec<HasamiToken> = tokens
            .into_iter()
            .map(|t| HasamiToken {
                surface: CString::new(t.surface.as_ref())
                    .unwrap_or_default()
                    .into_raw(),
                start: t.start as u32,
                end: t.end as u32,
                pos: CString::new(t.pos.as_ref()).unwrap_or_default().into_raw(),
                base_form: CString::new(t.base_form.as_ref())
                    .unwrap_or_default()
                    .into_raw(),
                reading: CString::new(t.reading.as_ref())
                    .unwrap_or_default()
                    .into_raw(),
                is_known: if t.is_known { 1 } else { 0 },
            })
            .collect();

        let ptr = ffi_tokens.as_mut_ptr();
        let capacity = ffi_tokens.capacity() as u32;
        std::mem::forget(ffi_tokens);

        HasamiTokenList {
            tokens: ptr,
            len: len as u32,
            capacity,
        }
    }
}

/// トークンリストを解放
///
/// # Safety
/// list は hasami_tokenize から返されたもの
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hasami_free_tokens(list: HasamiTokenList) {
    unsafe {
        if list.tokens.is_null() {
            return;
        }

        let tokens = Vec::from_raw_parts(list.tokens, list.len as usize, list.capacity as usize);
        for token in tokens {
            if !token.surface.is_null() {
                drop(CString::from_raw(token.surface));
            }
            if !token.pos.is_null() {
                drop(CString::from_raw(token.pos));
            }
            if !token.base_form.is_null() {
                drop(CString::from_raw(token.base_form));
            }
            if !token.reading.is_null() {
                drop(CString::from_raw(token.reading));
            }
        }
    }
}

/// アナライザーを解放
///
/// # Safety
/// handle は hasami_new から返されたもの
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hasami_free(handle: *mut HasamiAnalyzer) {
    unsafe {
        if !handle.is_null() {
            drop(Box::from_raw(handle));
        }
    }
}

/// 最後のエラーメッセージを取得
///
/// # Safety
/// handle は有効なポインタ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hasami_last_error(handle: *const HasamiAnalyzer) -> *const c_char {
    unsafe {
        if handle.is_null() {
            return ptr::null();
        }
        match &(*handle).last_error {
            Some(err) => err.as_ptr(),
            None => ptr::null(),
        }
    }
}
