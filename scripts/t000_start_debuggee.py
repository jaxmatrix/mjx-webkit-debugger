#!/usr/bin/env python3
"""Start fixture page + MiniBrowser with inspector server for T-000."""
import os
import signal
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PAGE = os.path.join(ROOT, "fixtures", "page")
MB = "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser"
ADDR = "127.0.0.1:2999"

procs = []


def cleanup(*_):
    for p in procs:
        try:
            p.terminate()
        except Exception:
            pass
    sys.exit(0)


def main():
    signal.signal(signal.SIGINT, cleanup)
    signal.signal(signal.SIGTERM, cleanup)

    http = subprocess.Popen(
        [sys.executable, "-m", "http.server", "8731", "--directory", PAGE],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    procs.append(http)
    time.sleep(0.4)

    env = os.environ.copy()
    env["WEBKIT_INSPECTOR_SERVER"] = ADDR
    mb = subprocess.Popen(
        [
            MB,
            "--enable-developer-extras=true",
            "http://127.0.0.1:8731/index.html",
        ],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    procs.append(mb)
    print(f"MiniBrowser pid={mb.pid} inspector={ADDR}", flush=True)
    print("http://127.0.0.1:8731/  (fixtures/page)", flush=True)
    # Give the web process time to register a target.
    time.sleep(2.0)
    print("ready", flush=True)
    mb.wait()


if __name__ == "__main__":
    main()
