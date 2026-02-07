import subprocess
import sys
from pathlib import Path


def test_proxy_latency_strict_mode_fails_without_endpoint() -> None:
    script = Path(__file__).parents[2] / "benchmarks" / "proxy_latency.py"
    result = subprocess.run(
        [
            sys.executable,
            str(script),
            "--url",
            "http://127.0.0.1:1/mcp/tools/call",
            "--iterations",
            "2",
            "--require-success",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 1
    assert "No successful responses." in result.stdout


def test_agentbench_eval_requires_inputs() -> None:
    script = Path(__file__).parents[2] / "eval" / "agentbench_eval.py"
    result = subprocess.run(
        [
            sys.executable,
            str(script),
            "--dataset",
            "/tmp/does-not-exist.jsonl",
            "--require-inputs",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 1
    assert "Dataset not found." in result.stdout
