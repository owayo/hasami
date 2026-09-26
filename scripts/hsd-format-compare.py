"""Compare all serialized token fields without storing multi-GB dump files.

Usage: python hsd-format-compare.py OLD_EVAL OLD_DICT NEW_EVAL NEW_DICT CORPUS
The eval binaries must be compiled from hsd-format-eval.rs.
"""

import hashlib
import subprocess
import sys

old_eval, old_dict, new_eval, new_dict, corpus = sys.argv[1:]
commands = [
    [old_eval, "dump", old_dict, corpus],
    [new_eval, "dump", new_dict, corpus],
]
processes = []
digest = hashlib.sha256()
size = 0
try:
    for command in commands:
        processes.append(subprocess.Popen(command, stdout=subprocess.PIPE))
    while True:
        left, right = [process.stdout.read(65536) for process in processes]
        if left != right:
            raise RuntimeError(f"token bytes differ in chunk starting at {size}")
        if not left:
            break
        digest.update(left)
        size += len(left)
    for process in processes:
        if process.wait() != 0:
            raise RuntimeError(f"dump failed: {process.args}")
    print(f"equal bytes={size} sha256={digest.hexdigest()}")
finally:
    for process in processes:
        if process.poll() is None:
            process.terminate()
            process.wait()
        process.stdout.close()
