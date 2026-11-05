"""Dump the Python binding's complete token fields in the Rust evaluator format.

Usage: python python-dump.py DICTIONARY CORPUS
Run with the freshly built wheel in the isolated smoke-test environment.
"""

import struct
import sys
from pathlib import Path

import hasami

assert Path(hasami.__file__).resolve().is_relative_to(Path(sys.prefix).resolve())
analyzer = hasami.Analyzer(sys.argv[1])
lines = Path(sys.argv[2]).read_text(encoding="utf-8").splitlines()
stream = sys.stdout.buffer
stream.write(struct.pack("<Q", len(lines)))
for line in lines:
    tokens = analyzer.tokenize(line)
    stream.write(struct.pack("<Q", len(tokens)))
    for token in tokens:
        stream.write(struct.pack("<QQ", token.start, token.end))
        for field in [
            "surface",
            "pos",
            "conj_type",
            "conj_form",
            "base_form",
            "reading",
            "pronunciation",
        ]:
            value = getattr(token, field).encode("utf-8")
            stream.write(struct.pack("<Q", len(value)))
            stream.write(value)
        stream.write(struct.pack("<hB", token.word_cost, int(token.is_known)))
