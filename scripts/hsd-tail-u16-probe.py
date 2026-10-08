"""Convert trusted v5 dictionaries to experimental u16-code TAIL dictionaries.

Usage: mise exec -- python scripts/hsd-tail-u16-probe.py OUTPUT DICTIONARY...
This is a measurement converter, not a migration tool or production writer.
All TAIL strings are round-tripped, and the other 16 sections stay identical.
"""

import hashlib
import json
import struct
import sys
from pathlib import Path

VERSION = 5007
LIMIT = 1 << 30


def varint(data, position):
    """Read a u32 LEB128 from a trusted input.

    Returns:
        The decoded value and the position after the varint.

    Raises:
        ValueError: If the varint overflows or has no terminator.

    """
    value = 0
    for shift in range(0, 35, 7):
        byte = data[position]
        position += 1
        if shift == 28 and byte & 127 > 15:
            raise ValueError("varint overflow")
        value |= (byte & 127) << shift
        if byte < 128:
            return value, position
    raise ValueError("unterminated varint")


def encoded_varint(value):
    """Encode a nonnegative u32 as LEB128.

    Returns:
        The encoded bytearray.

    """
    assert 0 <= value <= 0xFFFFFFFF
    out = bytearray()
    while value >= 128:
        out.append((value & 127) | 128)
        value >>= 7
    out.append(value)
    return out


def sections(data):
    """Read section payloads with basic trusted-file checks.

    Returns:
        The 18 section identifiers and their byte payloads in file order.

    """
    assert data[:8] == b"HSMDICT\0"
    assert struct.unpack_from("<Q", data, 24)[0] == len(data)
    count = struct.unpack_from("<I", data, 16)[0]
    assert count == 18
    result = []
    for index in range(count):
        sid, reserved, offset, length = struct.unpack_from(
            "<IIQQ", data, 64 + 24 * index
        )
        assert reserved == 0 and offset % 64 == 0 and offset + length <= len(data)
        assert sid not in [item[0] for item in result]
        result.append((sid, data[offset : offset + length]))
    return result


def convert(source, output):
    """Write an experimental dictionary without replacing the source.

    Returns:
        Sizes, hashes, and round-trip results for the dictionary.

    """
    data = source.read_bytes()
    assert struct.unpack_from("<I", data, 8)[0] == 5
    original = sections(data)
    payloads = dict(original)
    blocks = list(struct.iter_unpack("<H", payloads[2]))
    tables = list(struct.iter_unpack("<H", payloads[3]))
    code_of = {}
    char_of = {}
    for block, (table,) in enumerate(blocks):
        if table == 0:
            continue
        for low in range(256):
            code = tables[table * 256 + low][0]
            if code:
                char = chr(block * 256 + low)
                assert code not in char_of
                char_of[code] = char
                code_of[char] = code

    tails = payloads[5]
    packed = bytearray()
    offsets = {}
    cursor = 0
    count = 0
    while cursor < len(tails):
        old = cursor
        value = struct.unpack_from("<I", tails, cursor)[0]
        assert value < LIMIT
        length, body = varint(tails, cursor + 4)
        assert length and body + length <= len(tails)
        suffix = tails[body : body + length].decode("utf-8")
        codes = [code_of[char] for char in suffix]
        assert "".join(char_of[code] for code in codes) == suffix
        offsets[old] = len(packed)
        packed.extend(struct.pack("<I", value))
        packed.extend(encoded_varint(len(codes)))
        for code in codes:
            packed.extend(struct.pack("<H", code))
        cursor = body + length
        count += 1
    assert cursor == len(tails) and len(packed) < LIMIT

    nodes = bytearray(payloads[4])
    used = set()
    for position in range(0, len(nodes), 8):
        base, check = struct.unpack_from("<II", nodes, position)
        if check != 0xFFFFFFFF and base >> 30 == 3:
            offset = base & (LIMIT - 1)
            struct.pack_into("<I", nodes, position, 3 << 30 | offsets[offset])
            used.add(offset)
    assert used == set(offsets)
    payloads[4] = nodes
    payloads[5] = packed

    result = bytearray(data[:64])
    struct.pack_into("<I", result, 8, VERSION)
    result.extend(bytes(24 * len(original)))
    for index, (sid, _) in enumerate(original):
        result.extend(bytes((-len(result)) % 64))
        offset = len(result)
        body = payloads[sid]
        result.extend(body)
        struct.pack_into("<IIQQ", result, 64 + 24 * index, sid, 0, offset, len(body))
    struct.pack_into("<Q", result, 24, len(result))
    rewritten = dict(sections(result))
    for sid, body in original:
        if sid not in (4, 5):
            assert rewritten[sid] == body
    output.mkdir(parents=True, exist_ok=True)
    destination = output / source.name
    assert destination.resolve() != source.resolve()
    destination.write_bytes(result)
    return {
        "dictionary": source.stem,
        "v5_bytes": len(data),
        "u16_bytes": len(result),
        "v5_sha256": hashlib.sha256(data).hexdigest(),
        "u16_sha256": hashlib.sha256(result).hexdigest(),
        "tails": count,
        "v5_tail_bytes": len(tails),
        "u16_tail_bytes": len(packed),
        "all_tail_strings_roundtrip": True,
        "other_16_sections_equal": True,
    }


if __name__ == "__main__":
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    for filename in sys.argv[2:]:
        print(json.dumps(convert(Path(filename), Path(sys.argv[1]))), flush=True)
