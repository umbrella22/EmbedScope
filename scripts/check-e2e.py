#!/usr/bin/env python3
"""Exercise the CLI in separate processes against the software reference device."""
import hashlib
import json
from pathlib import Path
import select
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / "target/debug/escope"
OUT = ROOT / "target/validation"
OUT.mkdir(parents=True, exist_ok=True)
evidence = Path(tempfile.mkdtemp(prefix="cli-e2e-", dir=OUT))
server = subprocess.Popen([str(CLI), "--json", "simulate", "--listen", "127.0.0.1:0"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
commands = []

try:
    assert select.select([server.stdout], [], [], 10)[0], "reference device did not start"
    endpoint = json.loads(server.stdout.readline())["endpoint"]

    def run(*args, ok=True):
        command = [str(CLI), "--device", endpoint, "--json", *args]
        start = time.time()
        result = subprocess.run(command, capture_output=True, text=True, timeout=15)
        commands.append({"args":list(args), "exit":result.returncode, "seconds":time.time()-start, "stdout":result.stdout, "stderr":result.stderr})
        assert (result.returncode == 0) == ok, result.stderr
        return [json.loads(line) for line in result.stdout.splitlines()]

    info = run("info")[0]
    assert info["device"]["model"] == "software / synthetic"
    assert len(info["capabilities"]) == 6
    before = run("ui", "tree")[0]
    assert before[1]["name"] == "Counter: 0"
    run("display", "capture", str(evidence / "before.png"))
    run("input", "tap", "100", "140")
    after = run("ui", "tree")[0]
    assert after[1]["name"] == "Counter: 1"
    run("display", "capture", str(evidence / "after.png"))
    first = (evidence / "before.png").read_bytes()
    second = (evidence / "after.png").read_bytes()
    assert first[:8] == b"\x89PNG\r\n\x1a\n" and first != second
    logs = run("logs", "--count", "3")
    assert len(logs) == 3 and all(log["target"].startswith("reference.") for log in logs)
    metrics = run("metrics", "--count", "2", "--interval-ms", "20")
    assert len(metrics) == 2 and metrics[0]["psram_free"] is None
    assert run("crash", "show")[0]["present"] is False
    bundle = evidence / "bundle"
    run("diagnose", "--output", str(bundle), "--logs", "2")
    report = json.loads((bundle / "report.json").read_text())
    assert report["complete"] and len(report["logs"]) == 2
    assert report["ui_tree"][1]["name"] == "Counter: 1"
    run("input", "tap", "4294967295", "0", ok=False)
    run("display", "capture", str(evidence / "before.png"), ok=False)
    run("--timeout-ms", "500", "logs", "--count", "1", "--target", "no-such-target", ok=False)
    hashes = {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in evidence.glob("*.png")}
    receipt = {"passed":True,"source":"software-reference","physical_device":False,"commands":commands,"png_sha256":hashes}
    (evidence / "receipt.json").write_text(json.dumps(receipt, indent=2))
    print(json.dumps({"passed":True,"commands":len(commands),"evidence":str(evidence)}))
finally:
    server.terminate()
    try:
        server.wait(timeout=5)
    except subprocess.TimeoutExpired:
        server.kill()
        server.wait()
