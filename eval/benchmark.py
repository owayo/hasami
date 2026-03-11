"""Hasami vs MeCab vs Sudachi vs sudachi.rs 評価比較ベンチマーク."""

import subprocess
import time
from pathlib import Path

from tabulate import tabulate

# ---------------------------------------------------------------------------
# テスト文
# ---------------------------------------------------------------------------
TEST_SENTENCES = [
    "東京都に住んでいる人々が増えている。",
    "私は昨日の夜、友人と一緒に映画を観に行きました。",
    "人工知能の発展により、多くの産業が大きく変わりつつある。",
    "日本の四季は美しく、特に春の桜と秋の紅葉が有名です。",
    "彼女は大学で計算機科学を専攻し、卒業後はソフトウェアエンジニアとして働いている。",
    "環境問題に対する意識が高まり、再生可能エネルギーの導入が加速している。",
    "量子コンピュータの実用化に向けた研究開発が世界中で活発に行われている。",
    "この問題を解決するためには、政府と民間企業の連携が不可欠である。",
    "少子高齢化が進む日本では、労働力不足が深刻な社会問題となっている。",
    "国際宇宙ステーションでは、様々な科学実験が行われている。",
]

ITERATIONS_SPEED = 3000
WARMUP = 100

HASAMI_DICT = Path(__file__).parent / "hasami-dict.hsd"
HASAMI_BIN = Path(__file__).parent.parent / "target" / "release" / "hasami"

# sudachi.rs のパスを自動検出（隣接リポジトリまたは環境変数で指定可能）
_os = __import__("os")
SUDACHI_RS_ROOT = Path(
    _os.environ.get(
        "SUDACHI_RS_ROOT",
        str(Path(__file__).parent.parent.parent / "sudachi.rs"),
    )
)
SUDACHI_RS_BIN = SUDACHI_RS_ROOT / "target" / "release" / "sudachi"
# Windows 対応
if not SUDACHI_RS_BIN.exists() and (SUDACHI_RS_BIN.with_suffix(".exe")).exists():
    SUDACHI_RS_BIN = SUDACHI_RS_BIN.with_suffix(".exe")
if not HASAMI_BIN.exists() and (HASAMI_BIN.with_suffix(".exe")).exists():
    HASAMI_BIN = HASAMI_BIN.with_suffix(".exe")

# sudachi.rs の辞書パス (system.dic)
SUDACHI_RS_DICT = Path(_os.environ.get("SUDACHI_RS_DICT", ""))


# ===========================================================================
# エンジンセットアップ
# ===========================================================================


def _setup_mecab():
    """MeCab (fugashi) のセットアップ.

    Returns:
        fugashi.GenericTagger: MeCab tagger インスタンス.

    """
    import fugashi

    return fugashi.Tagger()


def _setup_sudachi():
    """Sudachi (Python) のセットアップ.

    Returns:
        sudachipy.Tokenizer: Sudachi tokenizer インスタンス.

    """
    from sudachipy import Dictionary

    dict_ = Dictionary()
    return dict_.create()


def _find_sudachi_rs_dict():
    """sudachi.rs の辞書パスを自動検出.

    Returns:
        Path | None: 辞書パス、見つからなければ None.

    """
    if SUDACHI_RS_DICT.exists():
        return SUDACHI_RS_DICT

    # 一般的なパスを探索
    candidates = [
        SUDACHI_RS_ROOT / "resources" / "system.dic",
        SUDACHI_RS_ROOT / "resources" / "system_core.dic",
        SUDACHI_RS_ROOT / "target" / "release" / "resources" / "system.dic",
        Path.home() / ".local" / "share" / "sudachi" / "system_core.dic",
    ]
    for p in candidates:
        if p.exists():
            return p
    return None


# ===========================================================================
# トークナイズ関数
# ===========================================================================


def _tokenize_mecab(tagger, text):
    """MeCab でトークナイズ.

    Returns:
        list: MeCab ノードのリスト.

    """
    return tagger.parseToNodeList(text)


def _mecab_tokens_to_list(nodes):
    """MeCab ノードをタプルリストに変換.

    Returns:
        list[tuple]: (surface, feature) のリスト.

    """
    return [(n.surface, n.feature) for n in nodes if n.surface]


def _tokenize_sudachi(tokenizer, text):
    """Sudachi (Python) でトークナイズ.

    Returns:
        list: Sudachi 形態素のリスト.

    """
    from sudachipy import Tokenizer as SudachiMode

    return tokenizer.tokenize(text, SudachiMode.SplitMode.C)


def _sudachi_tokens_to_list(morphemes):
    """Sudachi 形態素をタプルリストに変換.

    Returns:
        list[tuple]: (surface, pos) のリスト.

    """
    return [(m.surface(), ",".join(m.part_of_speech())) for m in morphemes]


def _tokenize_hasami_cli(text):
    """Hasami CLI 経由でトークナイズ.

    Returns:
        list[tuple]: (surface, feature) のリスト.

    """
    result = subprocess.run(
        [
            str(HASAMI_BIN),
            "tokenize",
            "--dict",
            str(HASAMI_DICT),
            text,
        ],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    tokens = []
    for line in result.stdout.strip().split("\n"):
        if line == "EOS" or not line.strip():
            continue
        parts = line.split("\t", 1)
        if len(parts) == 2:
            tokens.append((parts[0], parts[1]))
    return tokens


def _tokenize_sudachi_rs_cli(text, dict_path):
    """sudachi.rs CLI 経由でトークナイズ.

    Returns:
        list[tuple]: (surface, feature) のリスト.

    """
    cmd = [str(SUDACHI_RS_BIN)]
    if dict_path:
        cmd.extend(["-l", str(dict_path)])
    result = subprocess.run(
        cmd,
        input=text,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    tokens = []
    for line in result.stdout.strip().split("\n"):
        if line == "EOS" or not line.strip():
            continue
        parts = line.split("\t")
        if len(parts) >= 2:
            tokens.append((parts[0], parts[1] if len(parts) > 1 else ""))
    return tokens


# ===========================================================================
# ベンチマーク関数
# ===========================================================================


def _bench_mecab(tagger, sentences, iterations):
    """MeCab の速度ベンチマーク.

    Returns:
        float: 経過時間（秒）.

    """
    for _ in range(WARMUP):
        for s in sentences:
            tagger.parseToNodeList(s)

    start = time.perf_counter()
    for _ in range(iterations):
        for s in sentences:
            tagger.parseToNodeList(s)
    return time.perf_counter() - start


def _bench_sudachi(tokenizer, sentences, iterations):
    """Sudachi (Python) の速度ベンチマーク.

    Returns:
        float: 経過時間（秒）.

    """
    from sudachipy import Tokenizer as SudachiMode

    for _ in range(WARMUP):
        for s in sentences:
            tokenizer.tokenize(s, SudachiMode.SplitMode.C)

    start = time.perf_counter()
    for _ in range(iterations):
        for s in sentences:
            tokenizer.tokenize(s, SudachiMode.SplitMode.C)
    return time.perf_counter() - start


def _bench_hasami_native(sentences, iterations):
    """Hasami の Rust バイナリで直接ベンチマーク.

    Returns:
        dict: total_time と throughput を含む辞書.

    """
    total_time = 0.0
    total_sentences = iterations * len(sentences)

    for sentence in sentences:
        result = subprocess.run(
            [
                str(HASAMI_BIN),
                "bench",
                "--dict",
                str(HASAMI_DICT),
                "--text",
                sentence,
                "--iterations",
                str(iterations),
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        for line in result.stdout.strip().split("\n"):
            if "Total time:" in line:
                _, val = line.split(":", 1)
                total_time += float(val.strip().replace("s", ""))

    sps = total_sentences / total_time if total_time > 0 else 0
    return {"total_time": total_time, "throughput": sps}


def _bench_sudachi_rs_native(sentences, iterations, dict_path):
    """sudachi.rs の Rust バイナリで速度ベンチマーク.

    sudachi.rs に bench コマンドがないため、
    stdin パイプ経由でバッチ処理して計測する。

    Returns:
        dict: total_time と throughput を含む辞書.

    """
    # 全文を iterations 回繰り返した入力を作成
    input_text = "\n".join(sentences * iterations) + "\n"

    # ウォームアップ
    warmup_input = "\n".join(sentences * min(WARMUP, 10)) + "\n"
    cmd = [str(SUDACHI_RS_BIN)]
    if dict_path:
        cmd.extend(["-l", str(dict_path)])
    subprocess.run(
        cmd,
        input=warmup_input,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )

    # 本番計測
    start = time.perf_counter()
    result = subprocess.run(
        cmd,
        input=input_text,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    elapsed = time.perf_counter() - start

    if result.returncode != 0:
        print(f"  [WARN] sudachi.rs CLI returned code {result.returncode}")
        if result.stderr:
            print(f"  stderr: {result.stderr[:200]}")

    total_sentences = iterations * len(sentences)
    sps = total_sentences / elapsed if elapsed > 0 else 0
    return {"total_time": elapsed, "throughput": sps}


# ===========================================================================
# 品質比較
# ===========================================================================


def _compare_quality(text, mecab_tagger, sudachi_tokenizer, sudachi_rs_dict):
    """全エンジンの分割結果を比較.

    Returns:
        dict: エンジン名 -> [(surface, feature)] のマッピング.

    """
    results = {}

    mecab_nodes = _tokenize_mecab(mecab_tagger, text)
    results["MeCab"] = _mecab_tokens_to_list(mecab_nodes)

    sudachi_morphemes = _tokenize_sudachi(sudachi_tokenizer, text)
    results["Sudachi(py)"] = _sudachi_tokens_to_list(sudachi_morphemes)

    results["hasami"] = _tokenize_hasami_cli(text)

    if SUDACHI_RS_BIN.exists():
        results["sudachi.rs"] = _tokenize_sudachi_rs_cli(text, sudachi_rs_dict)

    return results


# ===========================================================================
# メイン
# ===========================================================================


def main():
    """評価比較のメインエントリポイント."""
    has_sudachi_rs = SUDACHI_RS_BIN.exists()
    sudachi_rs_dict = _find_sudachi_rs_dict() if has_sudachi_rs else None

    engines = "hasami vs MeCab vs Sudachi(py)"
    if has_sudachi_rs:
        engines += " vs sudachi.rs"

    print("=" * 78)
    print(f"  {engines} 評価比較ベンチマーク")
    print("=" * 78)
    print()

    if has_sudachi_rs:
        print(f"  sudachi.rs binary: {SUDACHI_RS_BIN}")
        print(f"  sudachi.rs dict:   {sudachi_rs_dict or '(auto-detect)'}")
    else:
        print(f"  [INFO] sudachi.rs not found at {SUDACHI_RS_BIN}")
        print(
            "         Set SUDACHI_RS_ROOT env var or build"
            " sudachi.rs with `cargo build --release`"
        )
    print()

    # --- [1/5] セットアップ ---
    print("[1/5] エンジン初期化...")

    t0 = time.perf_counter()
    mecab_tagger = _setup_mecab()
    mecab_init_time = time.perf_counter() - t0

    t0 = time.perf_counter()
    sudachi_tokenizer = _setup_sudachi()
    sudachi_init_time = time.perf_counter() - t0

    t0 = time.perf_counter()
    _tokenize_hasami_cli("テスト")
    hasami_init_time = time.perf_counter() - t0

    init_table = [
        ["MeCab (fugashi)", f"{mecab_init_time * 1000:.1f} ms"],
        [
            "Sudachi (Python)",
            f"{sudachi_init_time * 1000:.1f} ms",
        ],
        ["hasami (CLI)", f"{hasami_init_time * 1000:.1f} ms"],
    ]

    if has_sudachi_rs:
        t0 = time.perf_counter()
        _tokenize_sudachi_rs_cli("テスト", sudachi_rs_dict)
        sudachi_rs_init_time = time.perf_counter() - t0
        init_table.append(
            [
                "sudachi.rs (CLI)",
                f"{sudachi_rs_init_time * 1000:.1f} ms",
            ]
        )

    print("\n--- 初期化時間 ---")
    print(
        tabulate(
            init_table,
            headers=["エンジン", "初期化時間"],
            tablefmt="grid",
        )
    )

    # --- [2/5] 品質比較 ---
    print("\n[2/5] 分割品質比較...")
    print()

    for i, text in enumerate(TEST_SENTENCES[:5]):
        print(f"--- 文 {i + 1}: {text} ---")
        results = _compare_quality(
            text, mecab_tagger, sudachi_tokenizer, sudachi_rs_dict
        )

        for engine_name, tokens in results.items():
            surfaces = " | ".join([t[0] for t in tokens])
            print(f"  {engine_name:14s}: {surfaces}")
        print()

    # --- [3/5] 速度ベンチマーク (Python 経由) ---
    n_sent = len(TEST_SENTENCES)
    print(
        f"[3/5] 速度ベンチマーク (Python経由)"
        f" ({ITERATIONS_SPEED} iterations x {n_sent} sentences)..."
    )

    total_sentences = ITERATIONS_SPEED * n_sent

    mecab_time = _bench_mecab(mecab_tagger, TEST_SENTENCES, ITERATIONS_SPEED)
    mecab_sps = total_sentences / mecab_time

    sudachi_time = _bench_sudachi(sudachi_tokenizer, TEST_SENTENCES, ITERATIONS_SPEED)
    sudachi_sps = total_sentences / sudachi_time

    sudachi_ratio = f"{sudachi_sps / mecab_sps:.2f}x" if mecab_sps > 0 else "N/A"
    speed_table = [
        [
            "MeCab (fugashi)",
            f"{mecab_time:.3f} s",
            f"{mecab_sps:,.0f}",
            "1.00x (baseline)",
        ],
        [
            "Sudachi (Python)",
            f"{sudachi_time:.3f} s",
            f"{sudachi_sps:,.0f}",
            sudachi_ratio,
        ],
    ]

    print()
    print("--- Python バインディング速度比較 ---")
    print(
        tabulate(
            speed_table,
            headers=[
                "エンジン",
                "総時間",
                "sentences/sec",
                "比率 (vs MeCab)",
            ],
            tablefmt="grid",
        )
    )

    # --- [4/5] ネイティブ Rust ベンチマーク ---
    native_iterations = ITERATIONS_SPEED * n_sent
    print(
        f"\n[4/5] ネイティブ Rust 速度ベンチマーク"
        f" ({native_iterations:,} total iterations)..."
    )

    hasami_info = _bench_hasami_native(TEST_SENTENCES, native_iterations)
    hasami_sps = hasami_info["throughput"]
    hasami_time_native = hasami_info["total_time"]

    hasami_ratio = f"{hasami_sps / mecab_sps:.2f}x" if mecab_sps > 0 else "N/A"
    native_table = [
        [
            "hasami (Rust native)",
            f"{hasami_time_native:.3f} s",
            f"{hasami_sps:,.0f}",
            hasami_ratio,
        ],
    ]

    sudachi_rs_sps = 0
    if has_sudachi_rs:
        print("  sudachi.rs ベンチマーク中...")
        sudachi_rs_info = _bench_sudachi_rs_native(
            TEST_SENTENCES, ITERATIONS_SPEED, sudachi_rs_dict
        )
        sudachi_rs_sps = sudachi_rs_info["throughput"]
        sudachi_rs_time = sudachi_rs_info["total_time"]

        sr_ratio = f"{sudachi_rs_sps / mecab_sps:.2f}x" if mecab_sps > 0 else "N/A"
        native_table.append(
            [
                "sudachi.rs (Rust native)",
                f"{sudachi_rs_time:.3f} s",
                f"{sudachi_rs_sps:,.0f}",
                sr_ratio,
            ]
        )

    print()
    print("--- ネイティブ Rust 速度比較 ---")
    print(
        tabulate(
            native_table,
            headers=[
                "エンジン",
                "総時間",
                "sentences/sec",
                "比率 (vs MeCab)",
            ],
            tablefmt="grid",
        )
    )

    # hasami vs sudachi.rs 直接比較
    if has_sudachi_rs and sudachi_rs_sps > 0:
        print()
        print("--- hasami vs sudachi.rs 直接比較 ---")
        h_ratio = f"{hasami_sps / sudachi_rs_sps:.2f}x" if sudachi_rs_sps > 0 else "N/A"
        direct_table = [
            [
                "hasami",
                f"{hasami_sps:,.0f}",
                h_ratio,
            ],
            [
                "sudachi.rs",
                f"{sudachi_rs_sps:,.0f}",
                "1.00x (baseline)",
            ],
        ]
        print(
            tabulate(
                direct_table,
                headers=[
                    "エンジン",
                    "sentences/sec",
                    "比率 (vs sudachi.rs)",
                ],
                tablefmt="grid",
            )
        )

    # --- [5/5] 辞書・メモリ情報 ---
    print("\n[5/5] 辞書・メモリ情報...")

    result = subprocess.run(
        [str(HASAMI_BIN), "info", "--dict", str(HASAMI_DICT)],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    print("\n--- hasami 辞書情報 ---")
    print(result.stdout)

    hasami_dict_size = (
        HASAMI_DICT.stat().st_size / 1024 / 1024 if HASAMI_DICT.exists() else 0
    )

    size_table = [
        [
            "hasami",
            f"{hasami_dict_size:.1f} MB",
            "IPAdic (.hsd mmap-native)",
        ],
        [
            "MeCab (unidic-lite)",
            "~47 MB",
            "UniDic Lite (compiled)",
        ],
        [
            "Sudachi (core)",
            "~72 MB",
            "SudachiDict Core (.dic)",
        ],
    ]

    if has_sudachi_rs and sudachi_rs_dict and sudachi_rs_dict.exists():
        sr_size = sudachi_rs_dict.stat().st_size / 1024 / 1024
        size_table.append(
            [
                "sudachi.rs",
                f"{sr_size:.1f} MB",
                f"{sudachi_rs_dict.name}",
            ]
        )

    print("--- 辞書ファイルサイズ ---")
    print(
        tabulate(
            size_table,
            headers=["エンジン", "サイズ", "辞書"],
            tablefmt="grid",
        )
    )

    # --- サマリー ---
    print()
    print("=" * 78)
    print("  サマリー")
    print("=" * 78)
    print(f"  hasami:       {hasami_sps:>12,.0f} sentences/sec (native Rust)")
    print(f"  MeCab:        {mecab_sps:>12,.0f} sentences/sec (fugashi Python)")
    print(f"  Sudachi(py):  {sudachi_sps:>12,.0f} sentences/sec (sudachipy)")
    if has_sudachi_rs:
        print(f"  sudachi.rs:   {sudachi_rs_sps:>12,.0f} sentences/sec (native Rust)")
        print()
        if hasami_sps > 0 and sudachi_rs_sps > 0:
            print(
                f"  hasami は sudachi.rs の"
                f" {hasami_sps / sudachi_rs_sps:.1f} 倍、"
                f"MeCab の {hasami_sps / mecab_sps:.1f} 倍高速"
            )
    else:
        print()
        if hasami_sps > 0 and mecab_sps > 0:
            print(f"  hasami は MeCab の {hasami_sps / mecab_sps:.1f} 倍高速")
    print("=" * 78)


if __name__ == "__main__":
    main()
