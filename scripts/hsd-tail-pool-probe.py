"""Convert trusted v5 HSDs to the isolated UTF-8 TAIL pool experiment (5008).

Usage: mise exec -- python scripts/hsd-tail-pool-probe.py OUTPUT DICTIONARY...
This is a measurement converter, not a production writer or migration tool.
"""

import hashlib
import json
import struct
import sys
import time
from pathlib import Path

LIMIT = 1 << 30


def varint(data, cursor):
    """Decode a bounded u32 LEB128 from a trusted dictionary.

    Returns:
        The value and the position after the encoded integer.

    Raises:
        ValueError: If the varint has no terminator.

    """
    value = 0
    for shift in range(0, 35, 7):
        byte = data[cursor]
        cursor += 1
        assert shift != 28 or byte <= 15
        value |= (byte & 127) << shift
        if byte < 128:
            return value, cursor
    raise ValueError("unterminated varint")


def encode(value):
    """Encode a u32 as its shortest LEB128.

    Returns:
        The encoded bytes.

    """
    assert 0 <= value <= 0xFFFFFFFF
    out = bytearray()
    while value >= 128:
        out.append((value & 127) | 128)
        value >>= 7
    out.append(value)
    return out


def sections(data):
    """Read the 18 payloads, retaining section-table order.

    Returns:
        A mapping from section identifiers to payloads in file order.

    """
    assert data[:8] == b"HSMDICT\0"
    assert struct.unpack_from("<Q", data, 24)[0] == len(data)
    count = struct.unpack_from("<I", data, 16)[0]
    assert count == 18
    result = {}
    for index in range(count):
        sid, reserved, offset, length = struct.unpack_from(
            "<IIQQ", data, 64 + index * 24
        )
        assert reserved == 0 and offset % 64 == 0 and offset + length <= len(data)
        assert sid not in result
        result[sid] = data[offset : offset + length]
    return result


def convert(source, output):
    """Build a pooled file and directly verify every value and UTF-8 suffix.

    Returns:
        File identities, sizes, and results of the direct comparisons.

    """
    started = time.perf_counter()
    data = source.read_bytes()
    assert struct.unpack_from("<I", data, 8)[0] == 5
    original = sections(data)
    tails = original[5]
    records = []
    unique = {}
    pool = bytearray()
    cursor = 0
    while cursor < len(tails):
        old = cursor
        value = struct.unpack_from("<I", tails, cursor)[0]
        length, body = varint(tails, cursor + 4)
        assert value < LIMIT and length and body + length <= len(tails)
        suffix = tails[body : body + length]
        suffix.decode("utf-8")
        if suffix not in unique:
            unique[suffix] = len(pool)
            pool.extend(encode(length))
            pool.extend(suffix)
        records.append((old, value, unique[suffix]))
        cursor = body + length
    assert cursor == len(tails)
    pool_start = len(records) * 8
    packed = bytearray()
    offsets = {}
    for old, value, suffix_offset in records:
        offsets[old] = len(packed)
        packed.extend(struct.pack("<II", value, pool_start + suffix_offset))
    packed.extend(pool)
    assert len(packed) < LIMIT
    for old, value, _ in records:
        actual_value, pointer = struct.unpack_from("<II", packed, offsets[old])
        old_length, old_body = varint(tails, old + 4)
        new_length, new_body = varint(packed, pointer)
        assert actual_value == value and new_length == old_length
        assert (
            packed[new_body : new_body + new_length]
            == tails[old_body : old_body + old_length]
        )

    nodes = bytearray(original[4])
    references = set()
    for position in range(0, len(nodes), 8):
        base, check = struct.unpack_from("<II", nodes, position)
        if check != 0xFFFFFFFF and base >> 30 == 3:
            old = base & (LIMIT - 1)
            struct.pack_into("<I", nodes, position, (3 << 30) | offsets[old])
            references.add(old)
        else:
            assert (
                nodes[position : position + 8] == original[4][position : position + 8]
            )
        assert (
            nodes[position + 4 : position + 8]
            == original[4][position + 4 : position + 8]
        )
    assert references == set(offsets)

    payloads = {**original, 4: nodes, 5: packed}
    result = bytearray(data[:64])
    struct.pack_into("<I", result, 8, 5008)
    result.extend(bytes(24 * len(original)))
    for index, (sid, body) in enumerate(payloads.items()):
        result.extend(bytes(-len(result) % 64))
        struct.pack_into(
            "<IIQQ", result, 64 + index * 24, sid, 0, len(result), len(body)
        )
        result.extend(body)
    struct.pack_into("<Q", result, 24, len(result))
    rewritten = sections(result)
    assert all(
        rewritten[sid] == body for sid, body in original.items() if sid not in (4, 5)
    )
    output.mkdir(parents=True, exist_ok=True)
    destination = output / source.name
    assert destination.resolve() != source.resolve()
    with destination.open("xb") as stream:
        stream.write(result)
    return {
        "dictionary": source.stem,
        "v5_bytes": len(data),
        "pool_bytes": len(result),
        "v5_sha256": hashlib.sha256(data).hexdigest(),
        "pool_sha256": hashlib.sha256(result).hexdigest(),
        "tails": len(records),
        "unique_suffixes": len(unique),
        "v5_tail_bytes": len(tails),
        "pool_tail_bytes": len(packed),
        "records_bytes": pool_start,
        "strings_bytes": len(pool),
        "all_values_and_suffixes_equal": True,
        "other_16_sections_equal": True,
        "other_node_bytes_equal": True,
        "v5_sections_bytes": {sid: len(body) for sid, body in original.items()},
        "pool_sections_bytes": {sid: len(body) for sid, body in rewritten.items()},
        "conversion_wall_seconds": time.perf_counter() - started,
    }


if __name__ == "__main__":
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    for filename in sys.argv[2:]:
        print(json.dumps(convert(Path(filename), Path(sys.argv[1]))), flush=True)
