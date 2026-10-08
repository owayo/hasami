# 2026-10-03 の v5 辞書の再計測

基準コードは `1c7564a`（依存更新後に再ビルド）、形式は v5。入力は 2026-09-26 に作った既存の配布用辞書で、メタデータの作成版は `26.9.105`。辞書の内容と形式は今回変えていない。macOS 27.0.1、Apple Silicon、Rust 1.98.1、zstd 1.5.7 で測った。セクションごとの実バイト数は [sections.tsv](sections.tsv) に残す。

| 辞書 | SHA-256 | `.hsd` (B) | `zstd -19` (B) | 全件検証 | 短文 20,000 回の 1 文あたり |
| --- | --- | ---: | ---: | --- | ---: |
| ipadic | `5caa2462bc72e60addaece3352cef8fc7ed37ac6245427fd1c351228a7ea3a4b` | 16,493,282 | 5,827,281 | OK、0.07s | 1,602ns |
| ipadic-neologd | `34f2355f7138ff325bf8463c7909864f4dce17e7929a5d8ce005c6b78bfa11b2` | 206,570,094 | 66,917,486 | OK、1.25s | 1,766ns |
| ipadic-neologd-sudachi | `4682b59f8592779d5f300a48b91d1711ec6b9267b342ccd6190e1b9140879ee0` | 220,974,766 | 71,750,573 | OK、1.97s | 1,789ns |

`info --verify` は 3 辞書で全 trie・エントリ群・素性を検証した。短文は既定の「東京都に住んでいる人々が増えている。」で、各計測は約 0.03 秒しかない。速度の方式間比較や回帰の証拠には使わない。OS のページキャッシュが冷えた状態、最大 RSS、構築時間、混合コーパスは今回測っていない。

再現コマンド（リポジトリのルートで実行）:

```bash
make release
shasum -a 256 dict/ipadic.hsd dict/ipadic-neologd.hsd dict/ipadic-neologd-sudachi.hsd
for name in ipadic ipadic-neologd ipadic-neologd-sudachi; do
  stat -f '%N %z' "dict/$name.hsd"
  target/release/hasami info --dict "dict/$name.hsd" --verify
  target/release/hasami bench --dict "dict/$name.hsd" --iterations 20000
  zstd -q -19 -c "dict/$name.hsd" | wc -c
done
```
