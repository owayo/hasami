"""ローカル HTTP からの辞書取得を、変更前後の CLI を交互に実行して測る。"""

import argparse
import ctypes
import hashlib
import http.server
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import threading
import time


def process_metrics(process):
    """Windows の子プロセスの CPU 時間と最大 Working Set を取得する。"""
    if os.name != "nt":
        return {}

    class Counters(ctypes.Structure):
        _fields_ = [("cb", ctypes.c_ulong), ("faults", ctypes.c_ulong)] + [
            (name, ctypes.c_size_t)
            for name in (
                "peak_working_set",
                "working_set",
                "peak_paged",
                "paged",
                "peak_nonpaged",
                "nonpaged",
                "pagefile",
                "peak_pagefile",
            )
        ]

    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    handle = ctypes.c_void_p(int(process._handle))
    if not ctypes.windll.psapi.GetProcessMemoryInfo(
        handle, ctypes.byref(counters), counters.cb
    ):
        raise ctypes.WinError()
    times = [ctypes.c_ulonglong() for _ in range(4)]
    if not ctypes.windll.kernel32.GetProcessTimes(
        handle, *(ctypes.byref(value) for value in times)
    ):
        raise ctypes.WinError()
    return {
        "cpu_ms": (times[2].value + times[3].value) / 10_000,
        "peak_working_set_bytes": counters.peak_working_set,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--dict", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=9)
    args = parser.parse_args()
    if args.rounds < 2:
        parser.error("--rounds は 2 以上を指定する")

    binaries = {"before": args.before.resolve(), "after": args.after.resolve()}
    rows = []
    for dictionary in args.dict:
        payload = dictionary.read_bytes()
        digest = hashlib.sha256(payload).hexdigest()
        name = dictionary.stem
        catalog = json.dumps(
            {
                "hasami_version": "benchmark",
                "format_version": 5,
                "recommended": name,
                "dictionaries": [
                    {
                        "name": name,
                        "file": dictionary.name,
                        "size": len(payload),
                        "sha256": digest,
                    }
                ],
            }
        ).encode()

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                if self.path == "/dictionaries.json":
                    body = catalog
                elif self.path == f"/{dictionary.name}":
                    body = payload
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                view = memoryview(body)
                for start in range(0, len(view), 1 << 20):
                    self.wfile.write(view[start : start + (1 << 20)])

            def log_message(self, *_):
                pass

        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        url = f"http://127.0.0.1:{server.server_port}"
        try:
            with tempfile.TemporaryDirectory(prefix="hasami-download-bench-") as dest:
                for turn in range(-2, args.rounds):
                    order = (
                        ("before", "after") if turn % 2 == 0 else ("after", "before")
                    )
                    for variant in order:
                        command = [
                            str(binaries[variant]),
                            "dict",
                            "download",
                            name,
                            "--base-url",
                            url,
                            "--dir",
                            dest,
                            "--uncompressed",
                            "--force",
                            "--quiet",
                        ]
                        start = time.perf_counter_ns()
                        process = subprocess.Popen(
                            command, stdout=subprocess.PIPE, stderr=subprocess.PIPE
                        )
                        stdout, stderr = process.communicate()
                        elapsed = (time.perf_counter_ns() - start) / 1_000_000
                        if process.returncode:
                            raise RuntimeError(
                                (stdout + stderr).decode(errors="replace")
                            )
                        row = {
                            "dictionary": name,
                            "bytes": len(payload),
                            "sha256": digest,
                            "round": turn,
                            "variant": variant,
                            "wall_ms": elapsed,
                            **process_metrics(process),
                        }
                        # 計測後に保存した全バイトも確かめる。
                        with (Path(dest) / dictionary.name).open("rb") as saved:
                            assert (
                                hashlib.file_digest(saved, "sha256").hexdigest()
                                == digest
                            )
                        if turn >= 0:
                            rows.append(row)
                            print(json.dumps(row), flush=True)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8"
    )
    for name in dict.fromkeys(row["dictionary"] for row in rows):
        for variant in binaries:
            values = [
                row["wall_ms"]
                for row in rows
                if row["dictionary"] == name and row["variant"] == variant
            ]
            print(
                f"{name} {variant}: median={statistics.median(values):.3f} ms min={min(values):.3f} max={max(values):.3f}"
            )


if __name__ == "__main__":
    main()
