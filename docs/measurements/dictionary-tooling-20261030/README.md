# 2026-10-30 の定期点検と、辞書を全件なめる経路の計測

## 変更と採否

`Dictionary::verify`・`for_each_entry`・`surfaces` は、trie の全キーを集める
`Dictionary::groups` を通る。ここが `Trie::keys()` の `Vec<(u32, String)>` を受けていたため、
表層形ごとに `String` を割り当て、32 バイトの要素を群の先頭で並べ替え、最後に全部を解放していた。
推奨辞書では群が 4,713,155 個あるので、この割り当てと解放だけで数百万回になる。

`Trie::for_each_key(cb)` を足して、組み立て中の接頭辞の緩衝をそのまま貸すようにした
（末尾圧縮の節では末尾を継ぎ足して貸し、呼び出しの後で元に戻す）。`Trie::keys()` は
これを集めるだけの実装に変え、公開シグネチャは変えていない。`Dictionary::groups` は
表層形を 1 本の緩衝に連結し、`(群の先頭, 緩衝の位置, バイト長)` の 12 バイトの要素だけを
並べ替える `Groups` に変えた。位置と長さは u32 で持ち、表層形の総量が 4GiB を超える辞書は
`TrieError::Corrupt` で拒む（終わりの位置で確かめるので、位置と長さの両方が u32 に収まる）。

変更前に、`verify` の段階ごとの時間を一時的な計時で測った（推奨辞書、計測後に計時は除いた）。

| 段階 | 時間 |
| --- | ---: |
| `Trie::verify`（全ノード・全末尾レコード） | 337 ms |
| `groups()`（全キーの列挙と並べ替え） | 788 ms |
| 群の検証ループ（うち `trie.get` の引き直し 317 ms） | 662 ms |
| エントリの文脈 ID と素性レコードの走査 | 495 ms |

`groups()` が最大だったので、ここだけを変えた。群の検証ループにある `trie.get(key)` は
列挙と引き直しが一致することの検査なので残した。

リファクタリングは別に 2 件を実施した（計測の対象外。どちらも振る舞いを変えない）。

- `numeric::apply_counter_readings_with_protection`（168 行・循環的複雑度 50）を、
  数詞列の走査状態（`NumeralRun`）・直せる数詞かの判定（`numeral_before_counter`）・
  助数詞ごとの読みの適用（`apply_counter_reading`）に分けた。
  適用側は助数詞 × 数詞の語形の対応表なので、規則を散らさないため 1 つに保った。
- `dict::sentence_like::attaches`（123 行・複雑度 44）を、助動詞（`attaches_aux`）と
  助詞（`attaches_particle`）の判定に分けた。同じ形の `particle_follows` が既にある。

見送った候補も記録する。

- `lattice::LatticeWorkspace::tokenize_into_prepared`：ラティス構築・Viterbi・トレースバック・
  トークン生成を 1 回の走査で行う意図的な実装で、`self` の複数のフィールドを同時に
  可変借用している。関数に切り出すと借用を渡し直すことになり、
  [performance.md](../../performance.md) の最適化の前提を崩すため変えていない。
- `sentence::bracket` / `closing_bracket`、`lattice::unit_reading_multi` / `alpha_to_kana`、
  `numeric::numeral_glyph_readings` / `counter` / `ending`：複雑度は高いが、
  1 行 1 項目の対応表なので分割しない。
- 解析そのもの：[performance.md](../../performance.md) の 3 章に試して見送った案が並んでおり、
  今回新たに測って改善できる候補は見つけていない。

## 条件と再実行

- AMD Ryzen 7 5700X（8 コア・16 論理 CPU）、31.9 GiB、Windows 11 Home 10.0.26300 x64。
- Rust 1.98.1、Python 3.13.15、`cargo build --release`（opt-level=3、fat LTO、codegen-units=1）。
  両バイナリは同じ Cargo.lock を使う。依存の更新は今回採用していないので、差に含まれない。
- 変更前は `c62a211`、変更後はそこに今回の変更を当てたもの。
  リファクタリング 2 件も両方のバイナリに含めず、変更後だけに入っている
  （どちらも `groups` の経路には関わらない）。
- 辞書は同梱の `dict/ipadic.hsd`（390,860 エントリ）と
  `dict/ipadic-neologd-sudachi.hsd`（5,056,916 エントリ・4,713,155 群）。
- 各辞書・各操作でウォームアップを 1 回ずつ行った後、7 回ずつ測る。
  先に走るバイナリは回ごとに交替する。外れ値は除いていない。
- 毎回、出力が変更前後で一致することをスクリプトが確かめる
  （`export` は CSV 全体の SHA-256、`verify` は所要時間の行を除いた報告の SHA-256）。
- CPU 時間は子プロセスの `GetProcessTimes`（kernel + user）。最大メモリは
  `GetProcessMemoryInfo.PeakWorkingSetSize`。ページキャッシュは温まった条件で、
  CPU 固定はしていない。CPU 時間には 15.625 ms 単位の量子化がある。

```powershell
mise exec -- python scripts/bench-dictionary-tooling.py `
  --before <変更前のhasami.exe> --after <変更後のhasami.exe> `
  --dict dict/ipadic.hsd --dict dict/ipadic-neologd-sudachi.hsd `
  --rounds 7 --output docs/measurements/dictionary-tooling-20261030/tooling.jsonl
```

## 結果

中央値と［最小−最大］。全標本は [tooling.jsonl](tooling.jsonl)、集計値は
[summary.json](summary.json) にある。「重なり」は変更後の最大が変更前の最小を超えたか。

### IPAdic + NEologd + SudachiDict（5,056,916 エントリ）

| 指標 | 変更前 | 変更後 | 中央値の差 | 重なり |
| --- | ---: | ---: | ---: | :---: |
| `info --verify` CPU s | 2.109［2.078−2.312］ | 1.641［1.531−1.781］ | −22.2% | なし |
| `info --verify` 壁時計 s | 2.134［2.099−2.336］ | 1.664［1.538−1.831］ | −22.0% | なし |
| `info --verify` 最大 WS MiB | 547.035［547.000−547.082］ | 379.559［379.477−379.859］ | −30.6% | なし |
| `export` CPU s | 4.469［4.281−4.906］ | 3.938［3.781−4.531］ | −11.9% | あり |
| `export` 壁時計 s | 4.660［4.585−5.149］ | 4.185［4.065−5.009］ | −10.2% | あり |
| `export` 最大 WS MiB | 546.398［546.375−546.457］ | 358.531［358.453−358.562］ | −34.4% | なし |

### IPAdic（390,860 エントリ）

| 指標 | 変更前 | 変更後 | 中央値の差 | 重なり |
| --- | ---: | ---: | ---: | :---: |
| `info --verify` CPU s | 0.109［0.109−0.188］ | 0.094［0.078−0.141］ | −14.3% | あり |
| `info --verify` 壁時計 s | 0.123［0.120−0.142］ | 0.096［0.094−0.108］ | −22.0% | なし |
| `info --verify` 最大 WS MiB | 39.801［39.770−39.828］ | 29.016［28.977−29.328］ | −27.1% | なし |
| `export` CPU s | 0.281［0.266−0.312］ | 0.250［0.234−0.281］ | −11.1% | あり |
| `export` 壁時計 s | 0.280［0.278−0.312］ | 0.259［0.246−0.283］ | −7.6% | あり |
| `export` 最大 WS MiB | 36.551［36.504−36.582］ | 25.555［25.484−26.926］ | −30.1% | なし |

最大 Working Set は 4 条件すべてで範囲が重ならず、27〜34% 減った。
`verify` の時間は推奨辞書で CPU・壁時計とも範囲が重ならずに 22% 減った。
`export` の時間は中央値で 8〜12% 減ったが、全標本の範囲は重なる
（CSV の書き出しが時間の多くを占め、群の列挙の割合が `verify` より小さい）。
IPAdic の CPU は量子化の 1〜5 個分なので、11〜14% という比を高精度な改善率として扱わない。
変更を採用する。

### 測ったが採否に使わなかったもの

- `repair --drop-sentence-like-nouns`（`dict/ipadic-neologd.hsd`、`dict/ipadic.hsd` 参照）:
  CPU は 3 回ずつで 14.78〜15.36 s（変更前）と 13.78〜15.98 s（変更後）で範囲が重なり、
  最大 Working Set は 1,845 MiB と 1,837 MiB だった。辞書の作り直しが時間とメモリの
  ほとんどを占めるため、この経路の改善は確定しない。
- 解析（`bench --file`、8.15 MB・195,316 行のコーパス）: CPU 4.844 s と 4.891 s、
  最大 Working Set 35.9 MiB と 36.0 MiB で変化なし。解析は `groups` を通らない。
- `export-sentence-exceptions`（推奨辞書）: CPU 2.062 s → 1.922 s、
  最大 Working Set 546.5 MiB → 506.2 MiB。`surfaces()` は結果の `String` を作るので
  削減幅が小さい。抽出した 18,761 語は一致した。

## 検証

- `make SHELL=C:/PROGRA~1/Git/bin/sh.exe ci`：整形、ワークスペースと
  feature なし・`analyzer`・`download` の clippy / test を通過。テスト数は変更前と同じ。
- リファクタリングの振る舞いの同一性:
  - 解析: 10,803 行（リポジトリの Markdown + 数詞 × 助数詞の総当たり）を
    `tokenize --format json -j 1` で解析し、全フィールドの出力が
    `dict/ipadic.hsd`・`dict/ipadic-neologd-sudachi.hsd` のどちらでも SHA-256 一致。
  - 辞書の修復: `repair --drop-sentence-like-nouns` で 407,903 件の名詞を判定し、
    削除した 8 件とその理由が一致。出力辞書（197.0 MB・4,656,835 エントリ）も SHA-256 一致。
- 性能改善の出力の同一性: 上の計測スクリプトが毎回確かめたほか、
  `export` の CSV（2 辞書）、`info --verify` の報告、`export-sentence-exceptions` の
  18,761 語がいずれもバイト一致。
- 追加したテスト 3 件: 複数の保護範囲をまたぐ数詞列の音便、
  重複なしの表層形の一覧（メモリ上と mmap の両方。`surfaces()` にテストが無かった）、
  `for_each_key` の列挙と呼び出し側のエラーでの中断。
- `astro-sight impact --dir . --git` は 0 件。`review --dir . --git` の `api_changes` は空
  （`Trie::for_each_key` の追加だけで、既存の公開シグネチャは変えていない）。

## 依存とドキュメントの扱い

`depup --install --include-pinned` は Cargo（ルート・hasami-python）と `eval/pyproject.toml` で
更新なし（すべて最新）だった。mise のツールは今回いずれも採用していない。

- Rust 1.99.1: `mise.toml` の Rust は `Cargo.toml` の `rust-version`（MSRV 1.98）に
  そろえる取り決めなので、採用には MSRV を上げることになる。1.99 の機能を使っておらず、
  利用側の対応範囲を狭めるだけなので見送った。
- Python 3.15.1: `eval/pyproject.toml` の `requires-python = ">=3.12,<3.14"` に反する。
  範囲内の最新は 3.13.16 だが、mise が選ぶ python-build-standalone のビルドは 20261020
  （公開 10 日目）で、「公開から 14 日たった版を選ぶ」取り決めを満たさない。
- uv 0.13.2〜0.13.6: いずれも公開から 14 日未満（最も古い 0.13.2 が 11 日）。
- maturin 1.15.0 は最新。`biome.jsonc` は無いので `biome migrate` は対象外。

ドキュメント用の `/Users/owa/GitHub/mac-shell/shell/link_agent.md.sh` と同等のローカル
スクリプトはこの環境に無いため実行していない。`CLAUDE.md` は Git 上では AGENTS.md を指す
リンク（mode 120000）だが、この Windows checkout ではリンク先の文字列を持つ通常ファイルなので、
直接編集・置換していない。
