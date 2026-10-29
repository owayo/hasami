# 2026-10-29 の定期点検と辞書取得の計測

## 変更と採否

非圧縮の辞書取得では、同じバイト列を `Tee::read` と `Sink::write` がそれぞれ
SHA-256 に渡していた。最終照合に使うのは `Tee` の値だけだったため、書き込みを
`File::write_all` にしてハッシュの更新を1回にした。受信上限・進捗通知・ハッシュ照合は
引き続き `Tee` が担当し、書き込みエラーは従来どおり `Failure::Write` として返す。
圧縮取得では入力と展開結果が異なるため、両方のハッシュ計算を残している。

取得・圧縮入力・展開結果・持ち込みで重複していた「サイズ、SHA-256 の順で照合する」
処理を `check_integrity` にまとめた。4経路で検査順序やエラーの値がずれるのを防ぎ、
公開シグネチャ、エラー種別、404 のときだけ切り替える動作、一時ファイルからの配置を保った。

astro-sight の `symbols` で Rust と辞書用 Python の構造を確認し、次の候補も調べた。

- `hsd::writer::build_sections`：段階ごとの処理を持つ長い関数だが、
  エントリ順・共有表の番号・セクションの対応をまとめて扱う。
  今回は辞書形式や書き出しを変える根拠がなく、分割だけの変更は見送った。
- `cli::tokenize::BlockProcessor::process`：ワーカー・まとまり・出力順・最初のエラーを
  一緒に管理する。既存の入出力テストがあり、責務をさらに分ける効果より順序保証の変更リスクが大きい。
- 数詞の読み補正と括弧の分類：複雑度の上位には多数の規則を列挙する関数がある。
  数値だけで規則を分割せず、今回は重複した整合性検証の共通化を優先した。
- CSV 書き出し：13列の復元と書き込みエラーの保持を確認した。
  割り当て削減は今回計測した取得経路の改善に含めず、変更していない。

## 条件と再実行

- AMD Ryzen 7 5700X（8コア・16スレッド）、32 GiB、Windows 11 Home 10.0.26300。
- Rust 1.98.1、Python 3.13.15、`cargo build --release --locked`
  （opt-level=3、fat LTO、codegen-units=1）。両バイナリは同じ更新後の Cargo.lock を使用。
- 変更前は `78f6318` の取得実装に依存更新を適用したもの、変更後は `d02c963` の取得実装。
  依存更新の効果をこの差に含めない。
- `scripts/bench-dictionary-download.py` で、メモリに読み込んだ同一辞書を
  `127.0.0.1` の HTTP サーバーから配る。`dict download --uncompressed --force --quiet` を
  新しいプロセスで呼び、目録取得・ファイル書き込み・sync・形式検査・配置までを含む。
- 各辞書・各バイナリを2回ずつウォームアップした後、11回ずつ測る。
  先行するバイナリは組ごとに交替する。外れ値は除いていない。
- 毎回、配置後のファイル全体の SHA-256 が入力と一致することも計測外で確かめた。
  入力のサイズと SHA-256 は [download.jsonl](download.jsonl) に記録。
- CPU 時間は子プロセスの `GetProcessTimes`（kernel + user）。最大メモリは
  `GetProcessMemoryInfo.PeakWorkingSetSize` によるプロセス全体の最大 Working Set。
  HTTP サーバーの時間・メモリは含めない。ページキャッシュは温まった条件で、CPU 固定はしていない。
- 同時にほかのビルド・処理が動いていた。全体の CPU 使用率は未採取。
  CPU 時間にも15.625 ms単位の量子化があり、小さい差の精度には限界がある。

```powershell
mise exec -- python scripts/bench-dictionary-download.py `
  --before <変更前のhasami.exe> --after <変更後のhasami.exe> `
  --dict dict/ipadic.hsd --dict dict/ipadic-neologd.hsd `
  --dict dict/ipadic-neologd-sudachi.hsd `
  --rounds 11 --output download.jsonl
```

## 結果

中央値と IQR。全標本は [download.jsonl](download.jsonl)、集計値と範囲は
[summary.json](summary.json) にある。

| 辞書（非圧縮サイズ） | CPU ms 変更前 / 後 | CPU IQR ms 前 / 後 | CPU 中央値の差 | CPU が減った組 |
| --- | ---: | ---: | ---: | ---: |
| IPAdic（16,498,018 B） | 46.875 / 31.250 | 31.250 / 31.250 | −33.3% | 7 / 11 |
| + NEologd（206,569,122 B） | 515.625 / 375.000 | 109.375 / 31.250 | −27.3% | 11 / 11 |
| + SudachiDict（220,990,754 B） | 546.875 / 437.500 | 140.625 / 46.875 | −20.0% | 10 / 11 |

大きい2辞書では、CPU 中央値の差が IQR と同程度以上で、組ごとの比較でも10〜11組で減った。
同じバイト列へのハッシュ更新を2回から1回に減らす変更を採用する。
IPAdic の差は CPU 時間の計測単位1個分なので、33.3%という比を高精度な改善率として扱わない。

| 辞書 | 壁時計 ms 変更前 / 後 | 壁時計 IQR ms 前 / 後 | 最大 Working Set MiB 前 / 後 |
| --- | ---: | ---: | ---: |
| IPAdic | 917.535 / 657.721 | 536.669 / 774.806 | 7.273 / 7.277 |
| + NEologd | 4432.450 / 4546.245 | 2979.447 / 1645.040 | 7.277 / 7.297 |
| + SudachiDict | 5123.027 / 4100.710 | 3035.219 / 886.233 | 7.273 / 7.297 |

壁時計は待ち時間の変動が大きく、NEologd は中央値が2.6%増えた。
この計測から利用者の待ち時間の改善・退行を確定しない。
最大 Working Set の差は4〜24 KiBで、全標本の範囲も重なる。
圧縮取得の性能、低速回線、他OSの速度は未計測。

## 検証と環境の扱い

- `make SHELL=C:/PROGRA~1/Git/bin/sh.exe ci`：整形、ワークスペースと
  feature なし・analyzer・download の clippy / test を通過。
- 追加した2テストは、3バッファ以上の受信、末尾だけの破損、単調な進捗通知、
  読み取り専用ファイルへの書き込みエラーを確認。辞書取得の40テストも通過。
- 公開 v26.10.107 の配布辞書で `cargo test -- --ignored distributed` の33件を通過。
  最初は既存の古いローカル辞書で「先々週」の1件が失敗した。
  公開辞書は別の場所へ取得し、テスト時だけ入れ替え、元の3ファイルは SHA-256 一致を確認して復元した。
- Python の辞書ソース・入力テスト26件を通過。最初は System32 の WSL 起動用 Bash を選んで1件失敗し、
  Git for Windows の `bin` を PATH の先頭へ置いて再実行した。
- `cargo build --workspace` と maturin の CPython 3.13 wheel 作成・仮想環境へのインストールを通過。
  3辞書で CLI JSON の全フィールドと Python の解析・clone・batch の一致、空入力、
  存在しない辞書の例外、wheel の `Requires-Python: >=3.9` を確認した。
  Python 3.9 自体と他OSでの実行は未検証。
- `ruff check --select E9,F` と `cargo fmt --check`、最終の astro-sight impact を通過。
  解析アルゴリズム・辞書形式・Rust/C の公開 API は変えていない。

依存は memmap2 0.9.12、sha2 0.11.1、ureq 3.5.0、PyO3 と build-config 0.30.1、uv 0.13.1 へ更新。
[PyO3 の変更履歴](https://github.com/PyO3/pyo3/blob/v0.30.1/CHANGELOG.md) を確認し、
Python 3.8 の対応終了に合わせてパッケージと案内を3.9以上にした。
Rust は MSRV に合わせた1.98.1、Python は評価環境の `<3.14` に合わせた3.13.15を維持。
uv は公開から14日以上たった版を選び、mise.lock の5プラットフォーム分を再生成した。
depup による他OSの lock 情報の欠落は既報 [depup #42](https://github.com/owayo/depup/issues/42) と一致した。

影響分析では編集時の CRLF 化を戻すと未解決の影響は0件になった。
独立レビューでも公開宣言と呼び出し側の互換性を確認した。
解析ツール側の再現可能な誤検出は [astro-sight #177](https://github.com/owayo/astro-sight/issues/177)
（format! の暗黙キャプチャ）と [#178](https://github.com/owayo/astro-sight/issues/178)
（改行だけで不変の宣言が変更起点になる）へ報告した。

ドキュメント用の `/Users/owa/GitHub/mac-shell/shell/link_agent.md.sh` と同等のローカルスクリプトは
見つからなかったため実行していない。CLAUDE.md は Git 上で AGENTS.md を指すリンクだが、
この Windows checkout ではリンク先の文字列を持つ通常ファイルなので、直接編集・置換していない。
