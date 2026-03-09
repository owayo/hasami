//! C ABI (FFI) インターフェース
//!
//! C/C++ や他の言語からhasamiを利用するためのインターフェース

use crate::analyzer::Analyzer;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// FFI用のトークン
#[repr(C)]
pub struct HasamiToken {
    /// 表層形（UTF-8 のヌル終端文字列）
    pub surface: *mut c_char,
    /// 開始バイト位置
    pub start: u32,
    /// 終了バイト位置
    pub end: u32,
    /// 品詞情報（UTF-8 のヌル終端文字列）
    pub pos: *mut c_char,
    /// 原形（UTF-8 のヌル終端文字列）
    pub base_form: *mut c_char,
    /// 読み（UTF-8 のヌル終端文字列）
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

fn make_error_message(message: &str) -> CString {
    CString::new(message.replace('\0', " ")).unwrap_or_default()
}

fn set_thread_error(message: &str) {
    LAST_ERROR.with(|last_error| {
        *last_error.borrow_mut() = Some(make_error_message(message));
    });
}

fn clear_thread_error() {
    LAST_ERROR.with(|last_error| {
        *last_error.borrow_mut() = None;
    });
}

unsafe fn set_last_error(handle: *mut HasamiAnalyzer, message: &str) {
    let message = make_error_message(message);
    LAST_ERROR.with(|last_error| {
        *last_error.borrow_mut() = Some(message.clone());
    });
    if !handle.is_null() {
        unsafe {
            (*handle).last_error = Some(message);
        }
    }
}

unsafe fn clear_last_error(handle: *mut HasamiAnalyzer) {
    clear_thread_error();
    if !handle.is_null() {
        unsafe {
            (*handle).last_error = None;
        }
    }
}

/// 辞書ファイルからアナライザーを生成
///
/// # Safety
/// dict_path は有効なUTF-8 null終端文字列へのポインタ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hasami_new(dict_path: *const c_char) -> *mut HasamiAnalyzer {
    unsafe {
        if dict_path.is_null() {
            set_thread_error("dict_path が NULL です");
            return ptr::null_mut();
        }

        let path = match CStr::from_ptr(dict_path).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_thread_error("dict_path は UTF-8 文字列である必要があります");
                return ptr::null_mut();
            }
        };

        match Analyzer::load(path) {
            Ok(analyzer) => {
                clear_thread_error();
                Box::into_raw(Box::new(HasamiAnalyzer {
                    inner: analyzer,
                    last_error: None,
                }))
            }
            Err(e) => {
                let message = format!("辞書を読み込めません: {e}");
                set_thread_error(&message);
                eprintln!("hasami_new エラー: {}", e);
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

        if handle.is_null() {
            set_thread_error("handle が NULL です");
            return empty;
        }

        if text.is_null() {
            set_last_error(handle, "text が NULL です");
            return empty;
        }

        let analyzer = &mut (*handle).inner;
        let input = match CStr::from_ptr(text).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error(handle, "text は UTF-8 文字列である必要があります");
                return empty;
            }
        };

        let tokens = analyzer.tokenize(input);
        let len = tokens.len();

        if len == 0 {
            clear_last_error(handle);
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

        clear_last_error(handle);
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
        if !handle.is_null() {
            if let Some(err) = &(*handle).last_error {
                return err.as_ptr();
            }
        }
        LAST_ERROR.with(|last_error| {
            last_error
                .borrow()
                .as_ref()
                .map_or(ptr::null(), |err| err.as_ptr())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{DictBuilder, DictEntry};

    fn make_handle() -> *mut HasamiAnalyzer {
        let mut builder = DictBuilder::new();
        builder.add_entry(DictEntry {
            surface: "猫".into(),
            left_id: 1,
            right_id: 1,
            cost: 100,
            pos: "名詞,一般,*,*".into(),
            base_form: "猫".into(),
            reading: "ネコ".into(),
            pronunciation: "ネコ".into(),
        });
        let analyzer = Analyzer::from_dict(builder.build());
        Box::into_raw(Box::new(HasamiAnalyzer {
            inner: analyzer,
            last_error: None,
        }))
    }

    #[test]
    fn test_last_error_is_available_after_load_failure() {
        clear_thread_error();

        let missing = CString::new("/path/to/missing-dict.hsd").unwrap();
        let handle = unsafe { hasami_new(missing.as_ptr()) };
        assert!(handle.is_null());

        let error_ptr = unsafe { hasami_last_error(ptr::null()) };
        assert!(!error_ptr.is_null());

        let error = unsafe { CStr::from_ptr(error_ptr) }.to_str().unwrap();
        assert!(!error.is_empty());
    }

    #[test]
    fn test_last_error_is_set_and_cleared_by_tokenize() {
        clear_thread_error();

        let handle = make_handle();
        let invalid = CString::new(vec![0xFF]).unwrap();
        let empty = unsafe { hasami_tokenize(handle, invalid.as_ptr()) };
        assert_eq!(empty.len, 0);

        let error_ptr = unsafe { hasami_last_error(handle) };
        assert!(!error_ptr.is_null());
        let error = unsafe { CStr::from_ptr(error_ptr) }.to_str().unwrap();
        assert!(!error.is_empty());

        let text = CString::new("猫").unwrap();
        let tokens = unsafe { hasami_tokenize(handle, text.as_ptr()) };
        assert_eq!(tokens.len, 1);
        assert!(unsafe { hasami_last_error(handle) }.is_null());

        unsafe {
            hasami_free_tokens(tokens);
            hasami_free(handle);
        }
    }
}
