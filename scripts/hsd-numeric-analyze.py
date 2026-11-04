"""Measure numeral-prefixed keys and exact context-pair packing in trusted HSD v5.

Run after hsd-numeric-probe.rs: python scripts/hsd-numeric-analyze.py TRIAL DICT...
Only aggregates are saved to the tracked result. Extracted keys stay in TRIAL.
The 5009 files are storage prototypes, not supported dictionaries.
"""

import argparse
import array
import collections
import hashlib
import json
import re
import struct
import subprocess
import time
from itertools import pairwise
from pathlib import Path

PREFIX = re.compile(
    r"^([+\-＋－−]?[0-9０-９〇零一二三四五六七八九十百千万億兆]+"
    r"(?:[.,，．][0-9０-９〇零一二三四五六七八九十百千万億兆]+)*)(.*)$",
    re.DOTALL,
)
SUFFIXES = {
    "月",
    "日",
    "年",
    "人",
    "円",
    "個",
    "本",
    "枚",
    "台",
    "冊",
    "歳",
    "才",
    "回",
    "度",
    "分",
    "秒",
    "時",
    "時間",
    "日間",
    "週間",
    "か月",
    "ヶ月",
    "ケ月",
    "箇月",
    "年間",
    "周",
    "週",
    "件",
    "番",
    "号",
    "章",
    "節",
    "項",
    "点",
    "組",
    "階",
    "階建て",
    "隻",
    "匹",
    "頭",
    "羽",
    "杯",
    "足",
    "軒",
    "着",
    "段",
    "倍",
    "割",
    "俵",
    "丁",
    "通",
    "戸",
    "株",
    "勝",
    "敗",
    "位",
    "票",
    "曲",
    "校",
    "社",
    "店",
    "県",
    "都",
    "府",
    "市",
    "区",
    "町",
    "村",
    "歩",
    "面",
    "錠",
    "歯",
    "両",
    "機",
    "袋",
    "箱",
    "束",
    "cm",
    "km",
    "mm",
    "nm",
    "kg",
    "mg",
    "mL",
    "ml",
    "m",
    "g",
    "t",
    "L",
    "l",
    "W",
    "A",
    "V",
    "J",
    "N",
    "T",
    "F",
    "H",
    "K",
    "Hz",
    "kHz",
    "MHz",
    "GHz",
    "kW",
    "MW",
    "mA",
    "kV",
    "dB",
    "Pa",
    "hPa",
    "kPa",
    "MPa",
    "KB",
    "MB",
    "GB",
    "TB",
    "Wh",
    "kWh",
    "Ah",
    "mAh",
    "cc",
    "hp",
    "rpm",
    "fps",
    "bps",
    "Mbps",
    "Gbps",
    "bit",
    "mph",
    "kcal",
    "kcal/h",
    "km/h",
    "m/s",
    "mg/dL",
    "m2",
    "cm2",
    "km2",
    "m3",
    "m²",
    "cm²",
    "m³",
    "㎏",
    "㎞",
    "㎝",
    "㌔",
    "㌢",
    "％",
    "%",
    "℃",
    "°",
    "度目",
    "回目",
    "人目",
    "番目",
    "つ",
    "つ目",
    "馬身",
    "本塁打",
    "平方メートル",
    "立方メートル",
    "メートル",
    "キログラム",
    "キロメートル",
    "センチメートル",
    "ミリメートル",
    "グラム",
    "キロ",
}


def read_sections(data, version=5):
    """Read bounded section payloads from a trusted v5 container.

    Returns:
        Section IDs mapped to their byte payloads.

    """
    assert data[:8] == b"HSMDICT\0" and struct.unpack_from("<I", data, 8)[0] == version
    assert struct.unpack_from("<Q", data, 24)[0] == len(data)
    result = {}
    for index in range(struct.unpack_from("<I", data, 16)[0]):
        sid, reserved, offset, size = struct.unpack_from("<IIQQ", data, 64 + 24 * index)
        assert reserved == 0 and offset % 64 == 0 and offset + size <= len(data)
        assert sid not in result
        result[sid] = data[offset : offset + size]
    return result


def numeric_keys(path):
    """Decode the probe's keys and split their maximal numeric prefix.

    Returns:
        Tuples of group, full key, numeric spelling and suffix.

    """
    data = path.read_bytes()
    cursor = 0
    result = []
    while cursor < len(data):
        group, size = struct.unpack_from("<II", data, cursor)
        cursor += 8
        key = data[cursor : cursor + size].decode("utf-8")
        cursor += size
        prefix, suffix = PREFIX.fullmatch(key).groups()
        result.append((group, key, prefix, suffix))
    assert cursor == len(data)
    return result


def write_container(source, sections, target, version=5009):
    """Serialize all experimental sections with their full alignment costs.

    Returns:
        The complete serialized file size in bytes.

    """
    result = bytearray(source[:64])
    struct.pack_into("<I", result, 8, version)
    struct.pack_into("<I", result, 16, len(sections))
    result += bytes(24 * len(sections))
    for index, (sid, payload) in enumerate(sections.items()):
        result += bytes((-len(result)) % 64)
        struct.pack_into(
            "<IIQQ", result, 64 + index * 24, sid, 0, len(result), len(payload)
        )
        result += payload
    struct.pack_into("<Q", result, 24, len(result))
    target.write_bytes(result)
    # Verify the physical final container, including every section-table offset.
    actual = target.read_bytes()
    assert read_sections(actual, version) == sections
    assert actual[12:16] == source[12:16]
    return len(result)


def context_pairs(sections, source, target):
    """Share exact context-ID pairs and reconstruct every original entry.

    Returns:
        File sizes, identities and reconstruction results.

    """
    entries = sections[6]
    ids = {}
    table = bytearray()
    packed = bytearray()
    equal = 0
    for left_last, right, cost in struct.iter_unpack("<HHh", entries):
        left = left_last & 0x7FFF
        equal += left == right
        pair = (left, right)
        if pair not in ids:
            ids[pair] = len(ids)
            table += struct.pack("<HH", *pair)
        packed += struct.pack("<Hh", ids[pair] | (left_last & 0x8000), cost)
    assert len(ids) <= 32768, "prototype has no wide-ID fallback"
    # A complete, byte-for-byte reconstruction, including last-in-group and cost.
    restored = bytearray()
    for pair_last, cost in struct.iter_unpack("<Hh", packed):
        left, right = struct.unpack_from("<HH", table, (pair_last & 0x7FFF) * 4)
        restored += struct.pack("<HHh", left | (pair_last & 0x8000), right, cost)
    assert restored == entries
    changed = dict(sections)
    changed[6] = packed
    changed[19] = table
    size = write_container(source, changed, target, 5009)
    # Validate all unchanged payloads and the whole new section table.
    result = target.read_bytes()
    for index, (sid, payload) in enumerate(changed.items()):
        actual_sid, reserved, at, length = struct.unpack_from(
            "<IIQQ", result, 64 + index * 24
        )
        assert (
            actual_sid == sid and reserved == 0 and result[at : at + length] == payload
        )
    return {
        "pairs": len(ids),
        "equal_context_entries": equal,
        "entries_bytes": len(entries),
        "packed_bytes": len(packed),
        "table_bytes": len(table),
        "file_bytes": size,
        "saved_bytes": len(source) - size,
        "roundtrip_all_entries": True,
        "unchanged_sections_equal": True,
        "sha256": hashlib.sha256(result).hexdigest(),
    }


def symmetric_entries(sections, source, target):
    """Store one context ID only when every left/right pair is identical.

    Returns:
        File sizes, identity and the entry reconstruction result.

    """
    packed = bytearray()
    restored = bytearray()
    for left_last, right, cost in struct.iter_unpack("<HHh", sections[6]):
        assert left_last & 0x7FFF == right
        packed += struct.pack("<Hh", left_last, cost)
    for context_last, cost in struct.iter_unpack("<Hh", packed):
        restored += struct.pack("<HHh", context_last, context_last & 0x7FFF, cost)
    assert restored == sections[6]
    changed = dict(sections)
    changed[6] = packed
    size = write_container(source, changed, target, 5010)
    return {
        "file_bytes": size,
        "saved_bytes": len(source) - size,
        "roundtrip_all_entries": True,
        "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
    }


def factor_keys(selected, target):
    """Exact sparse (prefix,suffix)->original group mapping, with no cross product.

    Header: three u32 counts. String tables: (count+1) u32 byte offsets + UTF-8.
    Prefix row starts: (prefix_count+1) u32 record indices.
    Records per row, sorted by suffix ID: suffix_id u32 + original group u32.
    String IDs are never narrowed to u16. All original group values stay intact.

    Returns:
        Serialized bytes and the number of exactly reconstructed memberships.

    """
    prefixes = sorted({key[2] for key in selected})
    suffixes = sorted({key[3] for key in selected})
    pi = {s: i for i, s in enumerate(prefixes)}
    si = {s: i for i, s in enumerate(suffixes)}
    rows = [[] for _ in prefixes]
    for group, _, prefix, suffix in selected:
        rows[pi[prefix]].append((si[suffix], group))
    out = bytearray(struct.pack("<III", len(prefixes), len(suffixes), len(selected)))
    for strings in [prefixes, suffixes]:
        body = bytearray()
        positions = [0]
        for s in strings:
            body += s.encode("utf-8")
            positions.append(len(body))
        out += struct.pack(f"<{len(positions)}I", *positions) + body
    positions = [0]
    for row in rows:
        positions.append(positions[-1] + len(row))
    out += struct.pack(f"<{len(positions)}I", *positions)
    for row in rows:
        for suffix, group in sorted(row):
            out += struct.pack("<II", suffix, group)
    target.write_bytes(out)
    # Decode the serialized representation, not the source objects.
    raw = target.read_bytes()
    np, ns, nk = struct.unpack_from("<III", raw)
    at = 12
    decoded = []
    for n in [np, ns]:
        offsets = struct.unpack_from(f"<{n + 1}I", raw, at)
        at += 4 * (n + 1)
        decoded.append(
            [raw[at + a : at + b].decode("utf-8") for a, b in pairwise(offsets)]
        )
        at += offsets[-1]
    offsets = struct.unpack_from(f"<{np + 1}I", raw, at)
    at += 4 * (np + 1)
    reconstructed = []
    for prefix_id, (a, b) in enumerate(pairwise(offsets)):
        for index in range(a, b):
            suffix_id, group = struct.unpack_from("<II", raw, at + 8 * index)
            reconstructed.append((group, decoded[0][prefix_id] + decoded[1][suffix_id]))
    assert at + 8 * nk == len(raw)
    assert sorted(reconstructed) == sorted((g, key) for g, key, _, _ in selected)
    return {
        "bytes": len(out),
        "keys_roundtrip": len(reconstructed),
        "possible_cross_product": len(prefixes) * len(suffixes),
        "actual_memberships": len(selected),
    }


def analyze(trial, path, rebuild_tries=False):
    """Collect storage counts and create independently checked prototypes."""
    started = time.perf_counter()
    name = path.stem
    extracted = trial / f"{name}.keys"
    probe = subprocess.run(
        [str(trial / "probe.exe"), "probe", str(path), str(extracted)],
        check=True,
        capture_output=True,
        text=True,
    )
    counts = {
        k: int(v) for k, v in (line.split("\t") for line in probe.stdout.splitlines())
    }
    data = path.read_bytes()
    sections = read_sections(data)
    keys = numeric_keys(extracted)
    offsets = array.array("I", sections[7])
    refs = collections.Counter(offsets)
    starts = sorted(refs)
    lengths = {
        start: end - start
        for start, end in zip(starts, starts[1:] + [len(sections[8])], strict=True)
    }
    records = []
    for label, selected in [
        ("all_prefixed", keys),
        ("quantity_suffix", [k for k in keys if k[3] in SUFFIXES]),
        ("month", [k for k in keys if k[3] == "月"]),
    ]:
        selected_refs = collections.Counter()
        count = 0
        group_ids = []
        for group, _, _, _ in selected:
            group_ids.append(group)
            index = group
            while True:
                selected_refs[offsets[index]] += 1
                count += 1
                if struct.unpack_from("<H", sections[6], index * 6)[0] & 0x8000:
                    break
                index += 1
        drop_path = trial / f"{name}-{label}.drop"
        drop_path.write_bytes(b"".join(struct.pack("<I", group) for group in group_ids))
        exclusive = sum(
            lengths[offset] for offset, n in selected_refs.items() if refs[offset] == n
        )
        factor = factor_keys(selected, trial / f"{name}-{label}.factor")
        if rebuild_tries and label in {"quantity_suffix", "month"}:
            stat_path = trial / f"{name}-{label}.trie"
            subprocess.run(
                [
                    str(trial / "probe.exe"),
                    "trie",
                    str(path),
                    str(drop_path),
                    str(stat_path),
                ],
                check=True,
            )
            stats = {
                k: float(v) if k == "build_seconds" else int(v)
                for k, v in (
                    line.split("\t")
                    for line in stat_path.read_text(encoding="utf-8").splitlines()
                )
            }
            changed = dict(sections)
            for sid, extension in [
                (2, "blocks"),
                (3, "tables"),
                (4, "nodes"),
                (5, "tails"),
            ]:
                changed[sid] = stat_path.with_suffix(f".{extension}").read_bytes()
            changed[19] = (trial / f"{name}-{label}.factor").read_bytes()
            size = write_container(
                data, changed, trial / f"{name}-{label}-factor.hsd", 5011
            )
            factor.update(
                {
                    "file_bytes": size,
                    "saved_bytes": len(data) - size,
                    "remaining_trie": stats,
                    "sha256": hashlib.sha256(
                        (trial / f"{name}-{label}-factor.hsd").read_bytes()
                    ).hexdigest(),
                }
            )
        suffix_groups = collections.defaultdict(list)
        for group, _, _, suffix in selected:
            end = group
            while True:
                suffix_groups[suffix].append(
                    struct.unpack_from("<HHh", sections[6], end * 6)
                )
                if suffix_groups[suffix][-1][0] & 0x8000:
                    break
                end += 1
        records.append(
            {
                "category": label,
                "keys": len(selected),
                "entries": count,
                "unique_features": len(selected_refs),
                "exclusive_feature_bytes": exclusive,
                "entry_and_offset_bytes": count * 10,
                "distinct_prefixes": len({k[2] for k in selected}),
                "distinct_suffixes": len({k[3] for k in selected}),
                "top_suffix_counts": collections.Counter(
                    k[3] for k in selected
                ).most_common(20),
                "factor_prototype": factor,
                "single_cost_suffixes": sum(
                    len({v[2] for v in entries}) == 1
                    for entries in suffix_groups.values()
                ),
                "entry_cost_variants": len(
                    {v[2] for entries in suffix_groups.values() for v in entries}
                ),
            }
        )
    packed = context_pairs(sections, data, trial / f"{name}-pairs.hsd")
    symmetric = symmetric_entries(sections, data, trial / f"{name}-symmetric.hsd")
    result = {
        "dictionary": name,
        "file_bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "version": 5,
        "metadata": sections[1].decode("utf-8"),
        "sections": {str(k): len(v) for k, v in sections.items()},
        "counts": counts,
        "numeric": records,
        "pair_prototype": packed,
        "symmetric_prototype": symmetric,
        "probe_seconds": time.perf_counter() - started,
    }
    (trial / f"{name}.json").write_text(
        json.dumps(result, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(
        json.dumps(
            {k: v for k, v in result.items() if k not in {"metadata", "sections"}},
            ensure_ascii=True,
        ),
        flush=True,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trial", type=Path)
    parser.add_argument("dictionary", nargs="+", type=Path)
    parser.add_argument("--rebuild-tries", action="store_true")
    args = parser.parse_args()
    for dictionary in args.dictionary:
        analyze(args.trial.resolve(), dictionary.resolve(), args.rebuild_tries)
