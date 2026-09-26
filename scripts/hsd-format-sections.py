"""Compare all section payloads of two trusted HSDs, ignoring physical order.

Usage: python hsd-format-sections.py EXPECTED ACTUAL
Use the experimental converter to produce EXPECTED from the v4 rebuild.
"""

import hashlib
import json
import struct
import sys


def read_sections(f):
    """Read the comparison header and section ranges from a trusted file."""
    header = f.read(64)
    table = f.read(struct.unpack_from("<I", header, 16)[0] * 24)
    return header[:20], {
        key: (offset, size)
        for key, _, offset, size in struct.iter_unpack("<IIQQ", table)
    }


with open(sys.argv[1], "rb") as left, open(sys.argv[2], "rb") as right:
    lh, ls = read_sections(left)
    rh, rs = read_sections(right)
    assert lh == rh and ls.keys() == rs.keys(), "header or section IDs differ"
    result = {}
    for key, (offset, size) in ls.items():
        ro, rn = rs[key]
        assert size == rn, f"section {key}: length differs"
        left.seek(offset)
        right.seek(ro)
        digest = hashlib.sha256()
        remaining = size
        while remaining:
            chunk = left.read(min(1024 * 1024, remaining))
            assert chunk and chunk == right.read(len(chunk)), (
                f"section {key}: bytes differ"
            )
            digest.update(chunk)
            remaining -= len(chunk)
        result[key] = {"bytes": size, "sha256": digest.hexdigest()}
    print(json.dumps(result, sort_keys=True))
