//! 辞書モジュール - エントリ、接続コスト行列、辞書構築
//!
//! エントリ・未知語テンプレート・接続行列の型は解析側（[`crate::Dictionary`] の書き出し・取り込み）でも
//! 使うので常に入る。辞書を組み立てる [`DictBuilder`] と CSV の読み書きは `build` feature
//! （csv・encoding_rs・glob に依存）に閉じ込める。

use std::sync::Arc;

#[cfg(feature = "build")]
mod builder;
#[cfg(feature = "build")]
pub use builder::*;

/// 辞書エントリ（1形態素に対応）
///
/// 活用型・活用形が無い語は `*`（MeCab 形式 CSV と同じ）。空文字列は書き出し時に `*` にそろえる。
#[derive(Clone, Debug, Default)]
pub struct DictEntry {
    /// 表層形（Arc<str>で共有参照）
    pub surface: Arc<str>,
    /// 左文脈ID
    pub left_id: u16,
    /// 右文脈ID
    pub right_id: u16,
    /// 単語コスト
    pub cost: i16,
    /// 品詞情報（カンマ区切り）- Arc<str>で共有参照（クローンコスト最小）
    pub pos: Arc<str>,
    /// 活用型（MeCab 形式 CSV の 9 列目）
    pub conj_type: Arc<str>,
    /// 活用形（MeCab 形式 CSV の 10 列目）
    pub conj_form: Arc<str>,
    /// 原形
    pub base_form: Arc<str>,
    /// 読み
    pub reading: Arc<str>,
    /// 発音
    pub pronunciation: Arc<str>,
}

/// 未知語テンプレート
#[derive(Clone, Debug)]
pub struct UnkEntry {
    pub char_class: String,
    pub left_id: u16,
    pub right_id: u16,
    pub cost: i16,
    pub pos: String,
}

/// 接続コスト行列
///
/// matrix.def の 1 行目は「前の語の right_id の数」「次の語の left_id の数」、以降の行は
/// 「前の語の right_id」「次の語の left_id」「コスト」（MeCab と同じ）。
/// 次の語の left_id ごとに行を持つ転置形で保持する（v4 の MATRIX セクションと同じ並び）。
/// Viterbi は注目する語（left_id が決まる）を固定して前の語（right_id）を回すので、1 行の中で完結する。
#[derive(Clone, Debug)]
pub struct ConnectionMatrix {
    /// 次の語の left_id の数
    pub num_left: u32,
    /// 前の語の right_id の数
    pub num_right: u32,
    /// `costs[left_id * num_right + right_id]` = 接続コスト
    pub costs: Vec<i16>,
}

impl ConnectionMatrix {
    /// すべて 0 の行列
    pub fn zeros(num_left: u32, num_right: u32) -> Self {
        ConnectionMatrix {
            num_left,
            num_right,
            costs: vec![0; num_left as usize * num_right as usize],
        }
    }

    /// 文脈 ID がこの行列で引ける範囲にあるかどうか
    ///
    /// エントリの `left_id` は `num_left` 未満、`right_id` は `num_right` 未満でなければならない。
    /// 範囲外の ID を持つエントリは辞書に書き出せない（v3 までは接続コスト 0 として扱われ、
    /// そのエントリが不当に有利になっていた）。
    #[inline]
    pub fn contains_ids(&self, left_id: u16, right_id: u16) -> bool {
        (left_id as u32) < self.num_left && (right_id as u32) < self.num_right
    }

    /// 接続コスト（前の語の right_id → 次の語の left_id）。範囲外なら None
    #[inline]
    pub fn cost(&self, prev_right_id: u16, next_left_id: u16) -> Option<i16> {
        if !self.contains_ids(next_left_id, prev_right_id) {
            return None;
        }
        self.costs
            .get(next_left_id as usize * self.num_right as usize + prev_right_id as usize)
            .copied()
    }
}
