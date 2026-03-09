"""Hasami vs MeCab (fugashi) vs Sudachi 評価比較ベンチマーク."""

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


def _setup_mecab():
    """MeCab (fugashi) のセットアップ.

    Returns:
        fugashi.GenericTagger: MeCab tagger インスタンス.

    """
    import fugashi

    return fugashi.Tagger()


def _tokenize_mecab(tagger_full, text: str):
    """MeCab でトークナイズ.

    Returns:
        list: MeCab ノードのリスト.

    """
    return tagger_full.parseToNodeList(text)


def _mecab_tokens_to_list(nodes):
    """MeCab ノードをタプルリストに変換.

    Returns:
        list[tuple]: (surface, feature) のリスト.

    """
    return [(n.surface, n.feature) for n in nodes if n.surface]


def _setup_sudachi():
    """Sudachi のセットアップ.

    Returns:
        sudachipy.Tokenizer: Sudachi tokenizer インスタンス.

    """
    from sudachipy import Dictionary

    dict_ = Dictionary()
    return dict_.create()


def _tokenize_sudachi(tokenizer, text: str):
    """Sudachi でトークナイズ.

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


def _tokenize_hasami_cli(text: str):
    """Hasami CLI 経由でトークナイズ.

    Returns:
        list[tuple]: (surface, feature) のリスト.

    """
    result = subprocess.run(
        [str(HASAMI_BIN), "tokenize", "--dict", str(HASAMI_DICT), text],
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


def _bench_mecab(tagger_full, sentences, iterations):
    """MeCab の速度ベンチマーク.

    Returns:
        float: 経過時間（秒）.

    """
    for _ in range(WARMUP):
        for s in sentences:
            tagger_full.parseToNodeList(s)

    start = time.perf_counter()
    for _ in range(iterations):
        for s in sentences:
            tagger_full.parseToNodeList(s)
    return time.perf_counter() - start


def _bench_sudachi(tokenizer, sentences, iterations):
    """Sudachi の速度ベンチマーク.

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

    MeCab/Sudachi と同様に個別の文を処理するため、
    各文を個別にベンチマークして合計する。

    Returns:
        dict: ベンチマーク結果のキーバリュー辞書.

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
    info = {
        "Total time": f"{total_time:.3f}s",
        "Throughput": f"{sps:,.0f} sentences/sec",
    }
    return info


def _compare_quality(text, mecab_tagger, sudachi_tokenizer):
    """3つのエンジンの分割結果を比較.

    Returns:
        dict: エンジン名 -> [(surface, feature)] のマッピング.

    """
    mecab_nodes = _tokenize_mecab(mecab_tagger, text)
    mecab_tokens = _mecab_tokens_to_list(mecab_nodes)

    sudachi_morphemes = _tokenize_sudachi(sudachi_tokenizer, text)
    sudachi_tokens = _sudachi_tokens_to_list(sudachi_morphemes)

    hasami_tokens = _tokenize_hasami_cli(text)

    return {
        "mecab": mecab_tokens,
        "sudachi": sudachi_tokens,
        "hasami": hasami_tokens,
    }


def main():
    """評価比較のメインエントリポイント."""
    print("=" * 70)
    print("  hasami vs MeCab (fugashi) vs Sudachi 評価比較")
    print("=" * 70)
    print()

    # --- セットアップ ---
    print("[1/4] エンジン初期化...")

    t0 = time.perf_counter()
    mecab_full = _setup_mecab()
    mecab_init_time = time.perf_counter() - t0

    t0 = time.perf_counter()
    sudachi_tokenizer = _setup_sudachi()
    sudachi_init_time = time.perf_counter() - t0

    t0 = time.perf_counter()
    _tokenize_hasami_cli("テスト")
    hasami_init_time = time.perf_counter() - t0

    init_table = [
        ["MeCab (fugashi)", f"{mecab_init_time * 1000:.1f} ms"],
        ["Sudachi", f"{sudachi_init_time * 1000:.1f} ms"],
        ["hasami (CLI)", f"{hasami_init_time * 1000:.1f} ms"],
    ]
    print("\n--- 初期化時間 ---")
    print(
        tabulate(
            init_table,
            headers=["エンジン", "初期化時間"],
            tablefmt="grid",
        )
    )

    # --- 品質比較 ---
    print("\n[2/4] 分割品質比較...")
    print()

    for i, text in enumerate(TEST_SENTENCES[:5]):
        print(f"--- 文 {i + 1}: {text} ---")
        results = _compare_quality(text, mecab_full, sudachi_tokenizer)

        for engine_name, tokens in results.items():
            surfaces = " | ".join([t[0] for t in tokens])
            print(f"  {engine_name:10s}: {surfaces}")
        print()

    # --- 速度ベンチマーク ---
    n_sent = len(TEST_SENTENCES)
    print(
        f"[3/4] 速度ベンチマーク"
        f" ({ITERATIONS_SPEED} iterations × {n_sent} sentences)..."
    )

    total_sentences = ITERATIONS_SPEED * n_sent

    mecab_time = _bench_mecab(mecab_full, TEST_SENTENCES, ITERATIONS_SPEED)
    mecab_sps = total_sentences / mecab_time

    sudachi_time = _bench_sudachi(sudachi_tokenizer, TEST_SENTENCES, ITERATIONS_SPEED)
    sudachi_sps = total_sentences / sudachi_time

    hasami_info = _bench_hasami_native(TEST_SENTENCES, ITERATIONS_SPEED * n_sent)
    hasami_sps_str = hasami_info.get("Throughput", "N/A")
    hasami_total_str = hasami_info.get("Total time", "N/A")

    sudachi_ratio = f"{mecab_sps / sudachi_sps:.2f}x" if sudachi_sps > 0 else "N/A"
    speed_table = [
        [
            "MeCab (fugashi)",
            f"{mecab_time:.3f} s",
            f"{mecab_sps:,.0f}",
            "1.00x",
        ],
        [
            "Sudachi",
            f"{sudachi_time:.3f} s",
            f"{sudachi_sps:,.0f}",
            sudachi_ratio,
        ],
        ["hasami (native Rust)", hasami_total_str, hasami_sps_str, ""],
    ]

    try:
        hasami_sps_num = float(
            hasami_sps_str.replace(" sentences/sec", "").replace(",", "")
        )
        speed_table[2][3] = f"{hasami_sps_num / mecab_sps:.2f}x vs MeCab"
    except (ValueError, ZeroDivisionError):
        pass

    print()
    print("--- 速度比較 ---")
    print(
        tabulate(
            speed_table,
            headers=["エンジン", "総時間", "sentences/sec", "比率"],
            tablefmt="grid",
        )
    )

    # --- 辞書情報 ---
    print("\n[4/4] 辞書・メモリ情報...")

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

    print("--- 辞書ファイルサイズ ---")
    size_table = [
        ["hasami", f"{hasami_dict_size:.1f} MB", "IPAdic (postcard binary)"],
        ["MeCab (unidic-lite)", "~47 MB", "UniDic Lite (compiled)"],
        ["Sudachi (core)", "~72 MB", "SudachiDict Core"],
    ]
    print(
        tabulate(
            size_table,
            headers=["エンジン", "サイズ", "辞書"],
            tablefmt="grid",
        )
    )

    print("\n" + "=" * 70)
    print("  評価完了")
    print("=" * 70)


if __name__ == "__main__":
    main()
