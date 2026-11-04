"""Lossless numeric feature-template storage experiment, for trusted HSD v5 only.

Usage: python scripts/hsd-numeric-features.py TRIAL DICT...
Run hsd-numeric-analyze.py first. Experimental version 5012 is not a reader format.
"""

import array
import collections
import hashlib
import importlib.util
import json
import struct
import sys
from itertools import pairwise
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "numeric_analyze", Path(__file__).with_name("hsd-numeric-analyze.py")
)
analyze = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analyze)


def varint(data, at):
    """Read a bounded u32 varint from trusted bytes.

    Returns:
        The decoded value and the next byte offset.

    Raises:
        ValueError: If five bytes do not terminate the varint.

    """
    value = 0
    for shift in range(0, 35, 7):
        byte = data[at]
        at += 1
        assert shift != 28 or byte < 16
        value |= (byte & 127) << shift
        if byte < 128:
            return value, at
    raise ValueError("unterminated varint")


def encode(value):
    """Encode one u32 varint.

    Returns:
        The shortest unsigned LEB128 byte string.

    """
    assert 0 <= value <= 0xFFFFFFFF
    out = bytearray()
    while value >= 128:
        out.append((value & 127) | 128)
        value >>= 7
    out.append(value)
    return bytes(out)


def field(data, at):
    """Read one length-prefixed stored field.

    Returns:
        The stored byte string and its end offset.

    """
    length, at = varint(data, at)
    assert at + length <= len(data)
    return data[at : at + length], at + length


def common_tail(values):
    """Find the exact common byte suffix, capped at twelve bytes.

    Returns:
        The shared suffix, or an empty byte string.

    """
    tail = values[0][-12:]
    for value in values[1:]:
        while tail and not value.endswith(tail):
            tail = tail[1:]
    return tail


def blob_table(values):
    """Encode length-indexed byte strings; arbitrary bytes are permitted.

    Returns:
        The count, offset table and concatenated byte strings.

    """
    body = bytearray()
    offsets = [0]
    for value in values:
        body += value
        offsets.append(len(body))
    return struct.pack(f"<I{len(offsets)}I", len(values), *offsets) + body


def read_blob_table(data):
    """Decode a serialized table for the independent reconstruction check.

    Returns:
        The byte strings read through the stored offset table.

    """
    count = struct.unpack_from("<I", data)[0]
    offsets = struct.unpack_from(f"<{count + 1}I", data, 4)
    start = 4 + 4 * (count + 1)
    assert offsets[0] == 0 and start + offsets[-1] == len(data)
    return [data[start + a : start + b] for a, b in pairwise(offsets)]


def measure(trial, path):
    """Generate and verify a complete experimental container."""
    source = path.read_bytes()
    sections = analyze.read_sections(source)
    blob = sections[8]
    offsets = array.array("I", sections[7])
    starts = sorted(set(offsets))
    ends = dict(zip(starts, starts[1:] + [len(blob)], strict=True))
    groups = collections.defaultdict(set)
    fields = {}
    for group, _, _, suffix in analyze.numeric_keys(trial / f"{path.stem}.keys"):
        if suffix not in analyze.SUFFIXES:
            continue
        index = group
        while True:
            offset = offsets[index]
            grammar, flag_at = varint(blob, offset)
            flags = blob[flag_at]
            # Only base==surface and existing kana encodings. No linguistic rule.
            if flags & 1 and flags & 4 and (flags & 2 or flags & 8):
                reading, at = field(blob, flag_at + 1)
                pron = b""
                if not flags & 2:
                    pron, at = field(blob, at)
                assert at == ends[offset]
                fields[offset] = (grammar, flags, reading, pron)
                groups[(suffix, grammar, flags)].add(offset)
            if struct.unpack_from("<H", sections[6], index * 6)[0] & 0x8000:
                break
            index += 1
    candidates = {}
    for selected in groups.values():
        if len(selected) < 5:
            continue
        selected = sorted(selected)
        tail_r = common_tail([fields[offset][2] for offset in selected])
        tail_p = common_tail([fields[offset][3] for offset in selected])
        if not tail_r and not tail_p:
            continue
        for offset in selected:
            grammar, flags, reading, pron = fields[offset]
            prefix_r = reading[: len(reading) - len(tail_r)]
            prefix_p = pron[: len(pron) - len(tail_p)]
            candidate = (tail_r, tail_p, prefix_r, prefix_p)
            if offset not in candidates or len(tail_r) + len(tail_p) > sum(
                len(x) for x in candidates[offset][:2]
            ):
                candidates[offset] = candidate
    templates = sorted({parts[:2] for parts in candidates.values()})
    prefixes = sorted(
        {parts[2] for parts in candidates.values()}
        | {
            parts[3]
            for offset, parts in candidates.items()
            if not fields[offset][1] & 2
        }
    )
    template_ids = {pair: i for i, pair in enumerate(templates)}
    prefix_ids = {value: i for i, value in enumerate(prefixes)}
    new_blob = bytearray()
    remap = {}
    templated = 0
    for offset in starts:
        remap[offset] = len(new_blob)
        original = blob[offset : ends[offset]]
        if offset not in candidates:
            new_blob += original
            continue
        grammar, flags, _, _ = fields[offset]
        tail_r, tail_p, prefix_r, prefix_p = candidates[offset]
        packed = encode(grammar) + bytes([flags | 16])
        packed += encode(template_ids[(tail_r, tail_p)]) + encode(prefix_ids[prefix_r])
        if not flags & 2:
            packed += encode(prefix_ids[prefix_p])
        new_blob += packed
        templated += 1
    new_offsets = array.array("I", (remap[offset] for offset in offsets))
    prefix_data = blob_table(prefixes)
    template_data = blob_table(
        [encode(len(r)) + r + encode(len(p)) + p for r, p in templates]
    )
    # Decode the stored tables and each stored feature, then compare raw v5 bytes.
    decoded_prefixes = read_blob_table(prefix_data)
    decoded_templates = []
    for record in read_blob_table(template_data):
        r, at = field(record, 0)
        p, at = field(record, at)
        assert at == len(record)
        decoded_templates.append((r, p))
    decoded_count = 0
    for offset in starts:
        at = remap[offset]
        grammar, flag_at = varint(new_blob, at)
        flags = new_blob[flag_at]
        if flags & 16:
            template, cursor = varint(new_blob, flag_at + 1)
            prefix_r, cursor = varint(new_blob, cursor)
            tail_r, tail_p = decoded_templates[template]
            reading = decoded_prefixes[prefix_r] + tail_r
            restored = encode(grammar) + bytes([flags & ~16])
            restored += encode(len(reading)) + reading
            if not flags & 2:
                prefix_p, cursor = varint(new_blob, cursor)
                pron = decoded_prefixes[prefix_p] + tail_p
                restored += encode(len(pron)) + pron
            decoded_count += 1
        else:
            restored = bytes(new_blob[at : at + ends[offset] - offset])
        assert restored == blob[offset : ends[offset]]
    assert decoded_count == templated
    assert all(new == remap[old] for old, new in zip(offsets, new_offsets, strict=True))
    changed = dict(sections)
    changed[7] = new_offsets.tobytes()
    changed[8] = new_blob
    changed[19] = prefix_data
    changed[20] = template_data
    target = trial / f"{path.stem}-features.hsd"
    size = analyze.write_container(source, changed, target, 5012)
    result = {
        "dictionary": path.stem,
        "input_sha256": hashlib.sha256(source).hexdigest(),
        "file_bytes": size,
        "saved_bytes": len(source) - size,
        "features_before": len(blob),
        "features_after": len(new_blob),
        "prefix_pool_bytes": len(prefix_data),
        "template_bytes": len(template_data),
        "templates": len(templates),
        "prefixes": len(prefixes),
        "templated_features": templated,
        "all_unique_features_roundtrip": len(starts),
        "entry_offset_references_checked": len(offsets),
        "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
    }
    (trial / f"{path.stem}-features.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(json.dumps(result), flush=True)


if __name__ == "__main__":
    for dictionary in sys.argv[2:]:
        measure(Path(sys.argv[1]), Path(dictionary))
