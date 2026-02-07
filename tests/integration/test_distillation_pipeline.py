import json
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory


def test_distillation_pipeline_creates_artifacts() -> None:
    script = Path(__file__).parents[2] / "verifier" / "distillation" / "train.py"
    with TemporaryDirectory() as temp_dir:
        data_dir = Path(temp_dir) / "data"
        output_dir = Path(temp_dir) / "artifacts"
        result = subprocess.run(
            [
                sys.executable,
                str(script),
                "--data-dir",
                str(data_dir),
                "--output-dir",
                str(output_dir),
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        payload = json.loads(result.stdout)
        assert Path(payload["dataset"]).exists()
        assert Path(payload["metrics"]).exists()
