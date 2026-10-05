"""Recreate the literature input: AOZORA_ZIP OUTPUT (100 repetitions)."""

import hashlib
import re
import sys
import zipfile
from pathlib import Path

archive, output = map(Path, sys.argv[1:])
assert hashlib.sha256(archive.read_bytes()).hexdigest() == (
    "53c4d86033d0e4590c0354df5fec2709820ef21c09fa402826ff33376ec21228"
)
with zipfile.ZipFile(archive) as source:
    text = source.read("bocchan.txt").decode("shift_jis").replace("\r\n", "\n")
body = text.split("-------------------------------------------------------", 2)[2]
body, attribution = body.split("\n底本：", 1)
body = re.sub(r"《[^》]*》", "", body).replace("｜", "")
body = re.sub(r"［＃[^］]*］", "", body)
lines = [line.strip() for line in body.splitlines() if line.strip()]
data = ("\n".join(lines) + "\n").encode("utf-8")
assert len(lines) == 482 and len(data) == 265281
assert hashlib.sha256(data).hexdigest() == (
    "710c0ab73499adc3aec9df91c0e5926fc32cb2392b9e3d36c52e21443feea4db"
)
output.parent.mkdir(parents=True, exist_ok=True)
output.write_bytes(data * 100)
output.with_suffix(".NOTICE.txt").write_text(
    "坊っちゃん\n夏目漱石\n\n底本：" + attribution,
    encoding="utf-8",
)
print(f"lines={len(lines) * 100} bytes={len(data) * 100}")
print(f"sha256={hashlib.sha256(data * 100).hexdigest()}")
