import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
from datetime import datetime, timezone
from pathlib import Path

TOOLS = ("baseline", "reference", "candidate", "sourmash")
ORDERS = ((0, 1, 3, 2), (1, 2, 0, 3), (2, 3, 1, 0), (3, 0, 2, 1))


def sha256(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def write_json(path, value):
    with path.open("x") as handle:
        json.dump(value, handle, indent=2)
        handle.write("\n")


def usage(text, system):
    def one(pattern):
        values = re.findall(pattern, text, re.MULTILINE)
        if len(values) != 1:
            raise ValueError(f"missing or repeated resource field: {pattern}")
        return values[0]

    rss = int(one(r"^\s*(\d+)\s+maximum resident set size\s*$"))
    result = {
        "wall_seconds": float(one(r"^real\s+([0-9.]+)\s*$")),
        "user_seconds": float(one(r"^user\s+([0-9.]+)\s*$")),
        "system_seconds": float(one(r"^sys\s+([0-9.]+)\s*$")),
        "peak_rss_bytes": rss * (1024 if system == "Linux" else 1),
    }
    if result["wall_seconds"] < 0 or result["peak_rss_bytes"] <= 0:
        raise ValueError("invalid resource measurement")
    return result


def command(tool, program, input_path, output_path, abundance):
    if tool == "sourmash":
        parameter = "k=31,scaled=1000,seed=42," + ("abund" if abundance else "noabund")
        return [
            str(program),
            "sketch",
            "dna",
            "-p",
            parameter,
            "-o",
            str(output_path),
            str(input_path),
        ]
    return [
        str(program),
        "sketch",
        str(input_path),
        "--ksize",
        "31",
        "--scaled",
        "1000",
        "--seed",
        "42",
        *(["--abundance"] if abundance else []),
        "-o",
        str(output_path),
    ]


def trial(directory, arguments, system):
    directory.mkdir()
    stats = directory / "resources.txt"
    flags = (
        ["-lp"]
        if system == "Darwin"
        else [
            "-f",
            "real %e\nuser %U\nsys %S\n%M maximum resident set size",
        ]
    )
    timed = ["/usr/bin/time", *flags, "-o", str(stats), *arguments]
    write_json(directory / "command.json", timed)
    with (
        (directory / "stdout.txt").open("x") as stdout,
        (directory / "stderr.txt").open("x") as stderr,
    ):
        result = subprocess.run(timed, stdout=stdout, stderr=stderr, check=False)
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}); see {directory / 'stderr.txt'}"
        )
    return usage(stats.read_text(), system)


def main():
    parser = argparse.ArgumentParser(
        description="Record matched sketch CLI measurements without overwriting evidence"
    )
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--rounds", type=int, default=8)
    parser.add_argument("--abundance", action="store_true")
    for name in TOOLS:
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    system = platform.system()
    if system not in ("Linux", "Darwin"):
        parser.error("native Linux or macOS is required")
    if args.warmups < 1 or args.rounds < 4 or args.rounds % 4:
        parser.error(
            "require positive warmups and at least four measured rounds divisible by four"
        )
    input_path = args.input.resolve(strict=True)
    if sha256(input_path) != args.sha256:
        parser.error("input SHA-256 differs from the pinned fixture")
    programs = {name: getattr(args, name).resolve(strict=True) for name in TOOLS}
    output = args.output_dir.resolve()
    output.mkdir(parents=True)
    environment = {
        name: "1"
        for name in (
            "OMP_NUM_THREADS",
            "OPENBLAS_NUM_THREADS",
            "MKL_NUM_THREADS",
            "RAYON_NUM_THREADS",
        )
    }
    environment["LC_ALL"] = "C"
    os.environ.update(environment)
    affinity = None
    if system == "Linux":
        affinity = min(os.sched_getaffinity(0))
        os.sched_setaffinity(0, {affinity})
    write_json(
        output / "manifest.json",
        {
            "started_utc": datetime.now(timezone.utc).isoformat(),
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
            "cpu_affinity": affinity,
            "environment": environment,
            "input": str(input_path),
            "input_sha256": args.sha256,
            "input_bytes": input_path.stat().st_size,
            "abundance": args.abundance,
            "ksize": 31,
            "scaled": 1000,
            "seed": 42,
            "warmups": args.warmups,
            "rounds": args.rounds,
            "programs": {
                name: {"path": str(path), "sha256": sha256(path)}
                for name, path in programs.items()
            },
            "recorder_sha256": sha256(Path(__file__)),
            "time_sha256": sha256(Path("/usr/bin/time")),
            "resource_source": "native /usr/bin/time; Linux RSS converted from KiB to bytes",
        },
    )
    expected = None
    with (output / "observations.jsonl").open("x", buffering=1) as observations:
        for phase, rounds in (("warmup", args.warmups), ("measurement", args.rounds)):
            for round_index in range(rounds):
                for position, index in enumerate(ORDERS[round_index % len(ORDERS)]):
                    tool = TOOLS[index]
                    relative = Path(f"{phase}-{round_index:02d}-{tool}")
                    directory = output / relative
                    signature = directory / "signature.json"
                    arguments = command(
                        tool, programs[tool], input_path, signature, args.abundance
                    )
                    resources = trial(directory, arguments, system)
                    actual = signature.read_bytes()
                    if not actual:
                        raise ValueError(f"empty signature: {signature}")
                    if expected is None:
                        expected = actual
                    if actual != expected:
                        raise ValueError(f"output bytes differ: {signature}")
                    row = {
                        "phase": phase,
                        "round": round_index,
                        "position": position,
                        "tool": tool,
                        "command": arguments,
                        **resources,
                        "output": str(relative / "signature.json"),
                        "output_sha256": hashlib.sha256(actual).hexdigest(),
                        "resources": str(relative / "resources.txt"),
                    }
                    line = json.dumps(row)
                    observations.write(line + "\n")
                    print(line, flush=True)
    write_json(
        output / "complete.json",
        {
            "measured_trials": args.rounds * len(TOOLS),
            "output_sha256": hashlib.sha256(expected).hexdigest(),
            "performance_decision": "requires review of paired timing and RSS distributions",
        },
    )


if __name__ == "__main__":
    main()
