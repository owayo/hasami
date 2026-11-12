//! 補正の段と入力全体の下調べが発火しうる文字（錨）の有無を、1 回の走査で集める
//!
//! 最良パスに掛ける補正（数量・暦・段位・面積・成句など）と、その前に入力全体を調べる下調べ
//! （[`crate::numeric::AsciiPlan`]）は、どれも特定の文字を含む語や並びにしか当たらない。区間や入力全体に
//! その文字が無ければ何もしないので、呼ばずに済ませる。ここで集めるのは「この文字が無ければ何もしない」と
//! 言い切れる必要条件の材料だけで、条件そのものは各段・各下調べのモジュールが `may_apply` / `may_find` で持つ
//! （錨を足すときは、その条件と同じモジュールの規則の両方を確かめる）。
//!
//! 文字ごとの判定は分岐の予測に頼らないよう、BMP のページ（上位 8 ビット）と下位 8 ビットの 2 段の表を引く
//! （表は [`CHARS`] からコンパイル時に作る）。U+F000〜U+FFFF（UTF-8 の先頭バイトが 0xEF。全角の英数字・
//! 記号を含む）は表を使わず範囲で判定する。テストでは [`ungated`] の中だけ全ビットを立て、絞り込まない経路と
//! 結果が同じことを確かめる。

/// 錨の文字の有無の集合
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) struct Anchors(u32);

impl Anchors {
    /// ASCII の数字 `0`〜`9`
    pub(crate) const ASCII_DIGIT: Self = Self(1 << 0);
    /// U+F000〜U+FFFF（UTF-8 で 0xEF から始まる文字。全角の数字・英字・記号を含む）
    pub(crate) const FULLWIDTH: Self = Self(1 << 1);
    /// 漢数字の数字 `〇` `零` `一`〜`九`
    pub(crate) const KANJI_DIGIT: Self = Self(1 << 2);
    /// 漢数字の零 `〇` `零`（[`Self::KANJI_DIGIT`] にも含む）
    pub(crate) const KANJI_ZERO: Self = Self(1 << 3);
    /// 漢数字の位 `十` `百` `千` `万` `億` `兆`
    pub(crate) const KANJI_PLACE: Self = Self(1 << 4);
    /// `月`
    pub(crate) const MONTH: Self = Self(1 << 5);
    /// `年`
    pub(crate) const YEAR: Self = Self(1 << 6);
    /// `日`
    pub(crate) const DAY: Self = Self(1 << 7);
    /// `段`（段位）
    pub(crate) const DAN: Self = Self(1 << 8);
    /// `面`（面積）
    pub(crate) const MEN: Self = Self(1 << 9);
    /// `等`
    pub(crate) const NADO: Self = Self(1 << 10);
    /// `値`
    pub(crate) const ATAI: Self = Self(1 << 11);
    /// 成句の先頭の語の 1 字目 `真` `身` `得` `体` `間` `分`（`lattice::idiom` の規則の先頭）
    pub(crate) const IDIOM_HEAD: Self = Self(1 << 12);
    /// 全角の数字 `０`〜`９`（[`Self::FULLWIDTH`] にも含む）
    pub(crate) const WIDE_DIGIT: Self = Self(1 << 13);
    /// `外`（人の列挙）
    pub(crate) const HOKA: Self = Self(1 << 14);
    /// 全ビット（絞り込まない。テストの参照経路）
    #[cfg(test)]
    pub(crate) const ALL: Self = Self(u32::MAX);

    /// 2 つの集合の和（`const` の中でも使える）
    pub(crate) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// `other` のどれかの文字がある
    #[inline]
    pub(crate) const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// `other` の文字がすべてある
    #[inline]
    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// 1 文字の錨
    #[inline]
    pub(crate) fn of(c: char) -> Self {
        let cp = c as u32;
        let fullwidth = if (0xF000..=0xFFFF).contains(&cp) {
            Self::FULLWIDTH.0
        } else {
            0
        };
        let wide_digit = if ('０' as u32..='９' as u32).contains(&cp) {
            Self::WIDE_DIGIT.0
        } else {
            0
        };
        let page = if cp <= 0xFFFF {
            TABLE.index[(cp >> 8) as usize] as usize
        } else {
            0
        };
        Self(u32::from(TABLE.pages[page][(cp & 0xFF) as usize]) | fullwidth | wide_digit)
    }

    /// 文字列全体の錨（実際にある文字のとおり。絞り込みに使う前に [`Self::gated`] を通す）
    pub(crate) fn scan(text: &str) -> Self {
        let mut anchors = Self::default();
        for c in text.chars() {
            anchors.add(c);
        }
        anchors
    }

    /// 1 文字の錨を足す（[`Self::scan`] を別の走査の中で行うとき）
    #[inline]
    pub(crate) fn add(&mut self, c: char) {
        self.0 |= Self::of(c).0;
    }

    /// 新しく足した絞り込みに使う集合。テストの [`ungated`] の中では全ビットにして、絞り込まない
    /// 参照経路にする（絞り込む前からある早期の判定には、通す前の値を使う）
    #[inline]
    pub(crate) fn gated(self) -> Self {
        #[cfg(test)]
        if ungated_now() {
            return Self::ALL;
        }
        self
    }
}

/// 錨の文字と、その文字が持つ錨。U+F000〜U+FFFF（全角の数字を含む）は [`Anchors::of`] が範囲で
/// 足すので入れない
const CHARS: &[(char, Anchors)] = &[
    ('0', Anchors::ASCII_DIGIT),
    ('1', Anchors::ASCII_DIGIT),
    ('2', Anchors::ASCII_DIGIT),
    ('3', Anchors::ASCII_DIGIT),
    ('4', Anchors::ASCII_DIGIT),
    ('5', Anchors::ASCII_DIGIT),
    ('6', Anchors::ASCII_DIGIT),
    ('7', Anchors::ASCII_DIGIT),
    ('8', Anchors::ASCII_DIGIT),
    ('9', Anchors::ASCII_DIGIT),
    ('〇', Anchors::KANJI_DIGIT.union(Anchors::KANJI_ZERO)),
    ('零', Anchors::KANJI_DIGIT.union(Anchors::KANJI_ZERO)),
    ('一', Anchors::KANJI_DIGIT),
    ('二', Anchors::KANJI_DIGIT),
    ('三', Anchors::KANJI_DIGIT),
    ('四', Anchors::KANJI_DIGIT),
    ('五', Anchors::KANJI_DIGIT),
    ('六', Anchors::KANJI_DIGIT),
    ('七', Anchors::KANJI_DIGIT),
    ('八', Anchors::KANJI_DIGIT),
    ('九', Anchors::KANJI_DIGIT),
    ('十', Anchors::KANJI_PLACE),
    ('百', Anchors::KANJI_PLACE),
    ('千', Anchors::KANJI_PLACE),
    ('万', Anchors::KANJI_PLACE),
    ('億', Anchors::KANJI_PLACE),
    ('兆', Anchors::KANJI_PLACE),
    ('月', Anchors::MONTH),
    ('年', Anchors::YEAR),
    ('日', Anchors::DAY),
    ('段', Anchors::DAN),
    ('面', Anchors::MEN),
    ('等', Anchors::NADO),
    ('値', Anchors::ATAI),
    ('外', Anchors::HOKA),
    ('真', Anchors::IDIOM_HEAD),
    ('身', Anchors::IDIOM_HEAD),
    ('得', Anchors::IDIOM_HEAD),
    ('体', Anchors::IDIOM_HEAD),
    ('間', Anchors::IDIOM_HEAD),
    ('分', Anchors::IDIOM_HEAD),
];

/// 錨の文字があるページの数（0 番は錨の無いページ）
const PAGE_COUNT: usize = {
    let mut seen = [false; 256];
    let mut count = 1;
    let mut i = 0;
    while i < CHARS.len() {
        let page = (CHARS[i].0 as u32 >> 8) as usize;
        if !seen[page] {
            seen[page] = true;
            count += 1;
        }
        i += 1;
    }
    count
};

/// BMP の 2 段の表。`index[上位 8 ビット]` がページ、`pages[ページ][下位 8 ビット]` が錨
struct Table {
    index: [u8; 256],
    pages: [[u16; 256]; PAGE_COUNT],
}

static TABLE: Table = {
    let mut index = [0u8; 256];
    let mut pages = [[0u16; 256]; PAGE_COUNT];
    let mut next = 1;
    let mut i = 0;
    while i < CHARS.len() {
        let (c, anchors) = CHARS[i];
        let cp = c as u32;
        assert!(cp < 0xF000 && anchors.0 <= u16::MAX as u32);
        let page = (cp >> 8) as usize;
        if index[page] == 0 {
            index[page] = next;
            next += 1;
        }
        pages[index[page] as usize][(cp & 0xFF) as usize] |= anchors.0 as u16;
        i += 1;
    }
    Table { index, pages }
};

#[cfg(test)]
thread_local! {
    static UNGATED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// テストの [`ungated`] の中か。錨のほかの値で絞る箇所（範囲に掛かる単位の枠の有無、成句の先頭の語の表）が、
/// 参照経路では絞らずに通すのに使う。テスト以外ではいつも偽
#[inline]
pub(crate) fn ungated_now() -> bool {
    #[cfg(test)]
    {
        UNGATED.get()
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// `f` の中だけ、どの錨もあることにする（絞り込まない参照経路。テスト用）
#[cfg(test)]
pub(crate) fn ungated<R>(f: impl FnOnce() -> R) -> R {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            UNGATED.set(self.0);
        }
    }
    let _reset = Reset(UNGATED.replace(true));
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_listed_char_has_exactly_its_anchors() {
        for &(c, anchors) in CHARS {
            assert_eq!(Anchors::of(c), anchors, "{c}");
        }
    }

    #[test]
    fn unlisted_chars_have_no_anchors_except_the_fullwidth_ranges() {
        for cp in (0..=0x10FFFF).filter(|cp| !(0xD800..=0xDFFF).contains(cp)) {
            let c = char::from_u32(cp).unwrap();
            let expected = if let Some(&(_, anchors)) = CHARS.iter().find(|(l, _)| *l == c) {
                anchors
            } else if ('０'..='９').contains(&c) {
                Anchors::FULLWIDTH.union(Anchors::WIDE_DIGIT)
            } else if (0xF000..=0xFFFF).contains(&cp) {
                Anchors::FULLWIDTH
            } else {
                Anchors::default()
            };
            assert_eq!(Anchors::of(c), expected, "U+{cp:04X}");
        }
    }

    #[test]
    fn scan_collects_every_char_and_the_fullwidth_lead_byte() {
        let anchors = Anchors::scan("面積は３０平方メートル、2024年1月一日");
        for expected in [
            Anchors::MEN,
            Anchors::FULLWIDTH,
            Anchors::WIDE_DIGIT,
            Anchors::ASCII_DIGIT,
            Anchors::YEAR,
            Anchors::MONTH,
            Anchors::DAY,
            Anchors::KANJI_DIGIT,
        ] {
            assert!(anchors.contains(expected), "{expected:?}");
        }
        assert!(!anchors.intersects(Anchors::DAN.union(Anchors::NADO)));
        assert_eq!(Anchors::scan(""), Anchors::default());
        // 0xEF で始まる文字はどれも FULLWIDTH（既存の「0xEF のバイトがあるか」の判定と同じ）
        for text in ["\u{F000}", "（", "－", "ｱ", "\u{FFFD}"] {
            assert_eq!(text.as_bytes()[0], 0xEF);
            assert!(Anchors::scan(text).contains(Anchors::FULLWIDTH), "{text}");
            assert!(
                !Anchors::scan(text).intersects(Anchors::WIDE_DIGIT),
                "{text}"
            );
        }
        assert!(!Anchors::scan("\u{EFFF}𠀋").intersects(Anchors::FULLWIDTH));
    }

    #[test]
    fn ungated_sets_every_anchor_only_inside_the_closure() {
        assert_eq!(ungated(|| Anchors::scan("かな").gated()), Anchors::ALL);
        assert_eq!(
            ungated(|| ungated(|| Anchors::scan("").gated())),
            Anchors::ALL
        );
        // 実際の文字の集合は ungated の中でも変わらない
        assert_eq!(ungated(|| Anchors::scan("かな")), Anchors::default());
        assert_eq!(Anchors::scan("かな").gated(), Anchors::default());
        let mut anchors = Anchors::default();
        anchors.add('月');
        assert_eq!(anchors.gated(), Anchors::MONTH);
        assert!(!ungated_now());
        assert!(ungated(ungated_now));
        assert!(!ungated_now());
    }
}
