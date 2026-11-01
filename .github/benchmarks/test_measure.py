import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from measure import command, usage

SCRIPT = Path(__file__).with_name("measure.py")
TOOLS = ("baseline", "reference", "candidate", "sourmash")


class ResourceTests(unittest.TestCase):
    def test_native_rss_units_are_reported_as_bytes(self):
        text = "real 1.25\nuser 0.75\nsys 0.02\n2048 maximum resident set size\n"
        self.assertEqual(usage(text, "Linux")["peak_rss_bytes"], 2_097_152)
        self.assertEqual(usage(text, "Darwin")["peak_rss_bytes"], 2_048)
        self.assertEqual(usage(text, "Linux")["wall_seconds"], 1.25)

    def test_missing_or_duplicate_resource_fields_are_rejected(self):
        with self.assertRaises(ValueError):
            usage("real 1.25\nuser 0.75\nsys 0.02\n", "Linux")
        with self.assertRaises(ValueError):
            usage(
                "real 1.25\nreal 2.00\nuser 0.75\nsys 0.02\n2048 maximum resident set size\n",
                "Linux",
            )

    def test_abundance_and_default_profile_match_both_cli_contracts(self):
        self.assertEqual(
            command(
                "candidate", Path("rsomics"), Path("reads.fa"), Path("out.sig"), True
            ),
            [
                "rsomics",
                "sketch",
                "reads.fa",
                "--ksize",
                "31",
                "--scaled",
                "1000",
                "--seed",
                "42",
                "--abundance",
                "-o",
                "out.sig",
            ],
        )
        for abundance, profile in ((False, "noabund"), (True, "abund")):
            self.assertEqual(
                command(
                    "sourmash",
                    Path("sourmash"),
                    Path("reads.fa"),
                    Path("out.sig"),
                    abundance,
                ),
                [
                    "sourmash",
                    "sketch",
                    "dna",
                    "-p",
                    f"k=31,scaled=1000,seed=42,{profile}",
                    "-o",
                    "out.sig",
                    "reads.fa",
                ],
            )


class MeasurementTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.input = self.root / "reads.fa"
        self.input.write_bytes(b">read\nACGT\n")
        self.output = self.root / "evidence"
        self.tools = {}
        for name in TOOLS:
            program = self.root / name
            program.write_text(
                "#!/usr/bin/env python3\n"
                "from pathlib import Path\n"
                "import sys\n"
                "args = sys.argv[1:]\n"
                "assert args[0] == 'sketch'\n"
                "destination = Path(args[args.index('-o') + 1])\n"
                "destination.write_bytes(b'complete signature\\n')\n"
            )
            program.chmod(0o755)
            self.tools[name] = program

    def run_measurement(self, *extra):
        command = [
            sys.executable,
            "-B",
            str(SCRIPT),
            "--input",
            str(self.input),
            "--sha256",
            hashlib.sha256(self.input.read_bytes()).hexdigest(),
            "--output-dir",
            str(self.output),
            "--warmups",
            "1",
            "--rounds",
            "4",
        ]
        for name, program in self.tools.items():
            command.extend([f"--{name}", str(program)])
        return subprocess.run(
            [*command, *extra],
            capture_output=True,
            text=True,
            env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        )

    def test_retains_each_successful_trial_and_balances_positions(self):
        result = self.run_measurement()
        self.assertEqual(result.returncode, 0, result.stderr)
        rows = [
            json.loads(line)
            for line in (self.output / "observations.jsonl").read_text().splitlines()
        ]
        self.assertEqual(len(rows), 20)
        measured = [row for row in rows if row["phase"] == "measurement"]
        self.assertEqual(len(measured), 16)
        for name in TOOLS:
            self.assertEqual(
                sorted(row["position"] for row in measured if row["tool"] == name),
                [0, 1, 2, 3],
            )
        for row in rows:
            self.assertGreater(row["wall_seconds"], 0)
            self.assertGreater(row["peak_rss_bytes"], 0)
            self.assertEqual(
                (self.output / row["output"]).read_bytes(), b"complete signature\n"
            )
            self.assertTrue((self.output / row["resources"]).is_file())
        self.assertEqual(
            json.loads((self.output / "complete.json").read_text())["measured_trials"],
            16,
        )

    def test_product_profile_retains_two_balanced_roles_and_rejects_changed_bytes(self):
        self.tools = {name: self.tools[name] for name in ("candidate", "sourmash")}
        result = self.run_measurement("--profile", "product")
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = json.loads((self.output / "manifest.json").read_text())
        self.assertEqual(set(manifest["programs"]), {"candidate", "sourmash"})
        rows = [json.loads(line) for line in (self.output / "observations.jsonl").read_text().splitlines()]
        self.assertEqual(len(rows), 10)
        measured = [row for row in rows if row["phase"] == "measurement"]
        self.assertEqual(len(measured), 8)
        for name in ("candidate", "sourmash"):
            self.assertEqual(sorted(row["position"] for row in measured if row["tool"] == name), [0, 0, 1, 1])
        for index in range(4):
            self.assertEqual([row["tool"] for row in measured if row["round"] == index],
                             ["candidate", "sourmash"] if index % 2 == 0 else ["sourmash", "candidate"])
        self.assertEqual(json.loads((self.output / "complete.json").read_text())["measured_trials"], 8)
        for row in rows:
            self.assertEqual((self.output / row["output"]).read_bytes(), b"complete signature\n")
        self.tools["sourmash"].write_text(
            self.tools["sourmash"].read_text().replace("complete signature", "complete signature changed")
        )
        self.output = self.root / "changed-output"
        failed = self.run_measurement("--profile", "product")
        self.assertNotEqual(failed.returncode, 0)
        self.assertIn("output bytes differ", failed.stderr)
        self.assertFalse((self.output / "complete.json").exists())

    def test_existing_evidence_is_not_overwritten(self):
        self.output.mkdir()
        retained = self.output / "observations.jsonl"
        retained.write_bytes(b"existing evidence\n")
        result = self.run_measurement()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(retained.read_bytes(), b"existing evidence\n")
        self.assertEqual(list(self.output.iterdir()), [retained])

    def test_wrong_input_digest_stops_before_any_trial(self):
        result = self.run_measurement("--sha256", "0" * 64)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("input SHA-256", result.stderr)
        self.assertFalse(self.output.exists())

    def test_command_failure_retains_stderr_without_completion(self):
        self.tools["candidate"].write_text(
            "#!/bin/sh\necho failing-child >&2\nexit 17\n"
        )
        result = self.run_measurement()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.output / "complete.json").exists())
        logs = list(self.output.rglob("stderr.txt"))
        self.assertTrue(any("failing-child" in path.read_text() for path in logs))

    def test_different_signature_cannot_produce_completion(self):
        program = self.tools["candidate"]
        program.write_text(
            program.read_text().replace("complete signature", "different signature")
        )
        result = self.run_measurement()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("output bytes differ", result.stderr)
        self.assertFalse((self.output / "complete.json").exists())


if __name__ == "__main__":
    unittest.main()
