"""Trusted v4 input to experimental varint/fixed grammar layouts; not migration."""

import collections
import itertools
import struct
import sys
from pathlib import Path


def varint(data, p):
    n = shift = 0
    while True:
        c = data[p]
        p += 1
        n |= (c & 127) << shift
        if c < 128:
            return n, p
        shift += 7


def encoded(n):
    b = bytearray()
    while n >= 128:
        b.append((n & 127) | 128)
        n >>= 7
    b.append(n)
    return b


src, out, mode = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
data = src.read_bytes()
assert data[:8] == b"HSMDICT\0" and struct.unpack_from("<I", data, 8)[0] == 4
sections = {}
for i in range(struct.unpack_from("<I", data, 16)[0]):
    key, _, offset, length = struct.unpack_from("<IIQQ", data, 64 + i * 24)
    sections[key] = memoryview(data)[offset : offset + length]
blob = sections[8]
starts = []
freq = collections.Counter()
p = 0
while p < len(blob):
    starts.append(p)
    freq[struct.unpack_from("<HHH", blob, p)] += 1
    flags = blob[p + 6]
    p += 7
    for present in [True, not flags & 2, not flags & 1]:
        if present:
            n, p = varint(blob, p)
            p += n
assert p == len(blob)
starts.append(p)
grammar = sorted(freq, key=lambda t: (-freq[t], t))
ids = {t: i for i, t in enumerate(grammar)}
new = bytearray()
remap = {}
for start, end in itertools.pairwise(starts):
    remap[start] = len(new)
    gid = ids[struct.unpack_from("<HHH", blob, start)]
    new.extend(encoded(gid) if mode == "varint" else struct.pack("<H", gid))
    new.extend(blob[start + 6 : end])
sections[8] = new
sections[18] = b"".join(struct.pack("<HHH", *t) for t in grammar)
sections[7] = b"".join(
    struct.pack("<I", remap[o]) for (o,) in struct.iter_unpack("<I", sections[7])
)
header = bytearray(data[:64])
struct.pack_into("<I", header, 8, 5 if mode == "varint" else 5005)
struct.pack_into("<I", header, 16, len(sections))
position = 64 + 24 * len(sections)
table = bytearray()
for key, body in sections.items():
    position = (position + 63) & ~63
    table.extend(struct.pack("<IIQQ", key, 0, position, len(body)))
    position += len(body)
struct.pack_into("<Q", header, 24, position)
out.parent.mkdir(parents=True, exist_ok=True)
with out.open("wb") as f:
    f.write(header)
    f.write(table)
    for body in sections.values():
        f.write(b"\0" * (-f.tell() % 64))
        f.write(body)
assert out.stat().st_size == position
print(f"{src} -> {out}: {len(data)} -> {position} bytes, grammar={len(grammar)}")
