#!/usr/bin/env python3
"""Compare original Nucleo with a committed revision; write README-ready Markdown.

Requires Python 3.9+, Git, Cargo, and data from benches/prepare_data.py.
Both revisions run the working tree's matching benchmark in isolated Cargo packages.
Builds, the shared dependency lockfile, and raw results stay in target/bench-compare.
Progress goes to stderr, so stdout can be redirected to a Markdown file.
"""

import argparse
from datetime import datetime, timezone
import io
import json
import math
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BASELINE = "5b74652e482f7c07d827f18c6d21e7540c242c69"
CACHE = ROOT / "target" / "bench-compare"


def capture(command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def run(command, env):
    print(f"+ {shlex.join(map(str, command))}", file=sys.stderr, flush=True)
    subprocess.run(command, cwd=ROOT, env=env, stdout=sys.stderr, check=True)


def checkout(revision):
    """Export a commit without changing the checkout or registering a worktree."""
    commit = capture(["git", "rev-parse", "--verify", f"{revision}^{{commit}}"])
    sources = CACHE / "sources"
    sources.mkdir(parents=True, exist_ok=True)
    destination = sources / commit
    if not destination.exists():
        archive = subprocess.check_output(
            ["git", "archive", "--format=zip", commit], cwd=ROOT
        )
        with tempfile.TemporaryDirectory(dir=sources) as temporary:
            with zipfile.ZipFile(io.BytesIO(archive)) as files:
                files.extractall(temporary)
            Path(temporary).rename(destination)
    return commit, destination


def package_name(source):
    # These manifests declare [package] first; avoid a Python 3.11 TOML dependency.
    text = (source / "Cargo.toml").read_text(encoding="utf-8")
    return re.search(r'^name\s*=\s*"([^"]+)"', text, re.MULTILINE)[1]


def make_runner(workspace, name, source, fixtures):
    runner = workspace / name
    for relative in ["benches/matching.rs", "benches/support/mod.rs",
                     "benches/data/manifest.json", *fixtures]:
        destination = runner / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)
    # The exact same Rust source compiles against either package name via aliases.
    manifest = f'''[package]
name = "compare-{name}"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
ncp-engine = {{ package = {json.dumps(package_name(source))}, path = {json.dumps(str(source))} }}
ncp-matcher = {{ package = {json.dumps(package_name(source / "matcher"))}, path = {json.dumps(str(source / "matcher"))} }}
criterion = "=0.8.2"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
sha2 = "0.10"

[[bench]]
name = "matching"
harness = false
'''
    (runner / "Cargo.toml").write_text(manifest, encoding="utf-8")


def cases(size, threads):
    # Choose workloads by behavior, before looking at their timings.
    selected = [
        ("Score Chromium paths: `linux`", 1, "matcher/chromium/smart/selective"),
        ("Search Chromium paths: `src`", 1, "engine/chromium/threads_1/broad"),
    ]
    if threads != 1:
        selected.append(("Search Chromium paths: `src`", threads,
                         f"engine/chromium/threads_{threads}/broad"))
    selected.extend([
        ("Search Chromium paths: no match", threads,
         f"engine/chromium/threads_{threads}/no_match"),
        ("Append to Chromium query: `li` → `linux`", threads,
         f"editing/chromium/threads_{threads}/append"),
        ("Search German titles: `munchen`", threads,
         f"engine/wikipedia-de/threads_{threads}/normalization"),
        ("Search Japanese titles: `東京`", threads,
         f"engine/wikipedia-ja/threads_{threads}/selective"),
    ])
    return [(label, workers, f"{benchmark}/{size}")
            for label, workers, benchmark in selected]


def estimates(results, name, selected):
    values = {}
    for metadata in results.glob(f"**/{name}/benchmark.json"):
        benchmark = json.loads(metadata.read_text(encoding="utf-8"))["full_id"]
        estimate = json.loads(
            metadata.with_name("estimates.json").read_text(encoding="utf-8")
        )["mean"]["point_estimate"]
        if benchmark in values or not math.isfinite(estimate) or estimate <= 0:
            raise ValueError(f"Invalid or duplicate estimate for {name}: {benchmark}")
        values[benchmark] = estimate
    expected = {benchmark for _, _, benchmark in selected}
    if values.keys() != expected:
        raise ValueError(f"Unexpected benchmark results for {name}: "
                         f"missing {expected - values.keys()}, extra {values.keys() - expected}")
    return values


def machine():
    if platform.system() == "Darwin":
        cpu = capture(["sysctl", "-n", "machdep.cpu.brand_string"])
        return f"{cpu}, macOS {platform.mac_ver()[0]} ({platform.machine()})"
    return f"{platform.processor() or platform.machine()}, {platform.platform()}"


def markdown(args, commits, selected, original, current):
    lines = [
        "### Selected comparison with original Nucleo",
        "",
        f"Original Nucleo `{commits[0][:12]}` vs. ncp-engine `{commits[1][:12]}`; "
        f"{int(args.size):,} candidates per corpus.",
        f"Measured on {datetime.now(timezone.utc).date()}: {machine()}; "
        f"`{capture(['rustc', '--version'])}`.",
        f"Criterion means, {args.sample_size} samples, {args.warm_up_time:g} s warm-up "
        f"and {args.measurement_time:g} s measurement target per case; lower time is better.",
        "",
        "| Workload | Workers | Nucleo | ncp-engine | Speedup |",
        "| :--- | ---: | ---: | ---: | ---: |",
    ]
    for label, workers, benchmark in selected:
        old, new = original[benchmark], current[benchmark]
        lines.append(f"| {label} | {workers} | {old / 1e6:.3f} ms | "
                     f"{new / 1e6:.3f} ms | {old / new:.2f}× |")
    lines.extend([
        "",
        "Speedup is Nucleo time / ncp-engine time. Both revisions use the same harness, "
        "compiler, dependency lockfile, and checksum-verified corpus samples.",
        "Searches include scoring, sorting, and snapshot publication; data loading, "
        "injection, and query-reset setup are outside the timed region. "
        "Append measures filtering the preceding query's matches. "
        "Scoring uses each revision's defaults, with path matching enabled for Chromium "
        "and smart case matching/normalization throughout. "
        "The no-match query is `ncp927missingcandidate927`.",
        "",
    ])
    flags = os.environ.get("CARGO_ENCODED_RUSTFLAGS", os.environ.get("RUSTFLAGS", ""))
    if flags:
        lines.extend([f"Additional compiler flags: `{flags.replace(chr(31), ' ')}`.", ""])
    return "\n".join(lines)


def positive_int(value):
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def positive_seconds(value):
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise argparse.ArgumentTypeError("must be finite and positive")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--current", default="HEAD", help="Current Git revision (default: HEAD; excludes uncommitted edits)")
    parser.add_argument("--size", choices=["10000", "100000"], default="100000")
    parser.add_argument("--threads", type=positive_int, default=min(8, os.cpu_count() or 1),
                        help="Worker count for parallel cases (default: up to 8); also measures a 1-worker search")
    parser.add_argument("--sample-size", type=positive_int, default=20)
    parser.add_argument("--warm-up-time", type=positive_seconds, default=0.5)
    parser.add_argument("--measurement-time", type=positive_seconds, default=2.0)
    args = parser.parse_args()
    if args.sample_size < 10:
        parser.error("--sample-size must be at least 10 (Criterion's minimum)")

    manifest = json.loads((ROOT / "benches/data/manifest.json").read_text(encoding="utf-8"))
    fixtures = [f"benches/data/prepared/{source['name']}.txt" for source in manifest["datasets"]]
    for fixture in fixtures:
        if not (ROOT / fixture).is_file():
            raise FileNotFoundError(f"Missing {fixture}; run `python3 benches/prepare_data.py` first")

    baseline, baseline_source = checkout(BASELINE)
    current, current_source = checkout(args.current)
    workspace = CACHE / "runner"
    workspace.mkdir(parents=True, exist_ok=True)
    (workspace / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["original", "current"]\nresolver = "2"\n', encoding="utf-8"
    )
    make_runner(workspace, "original", baseline_source, fixtures)
    make_runner(workspace, "current", current_source, fixtures)
    # One workspace resolves shared dependencies to the same versions for both builds.
    cargo = ["cargo", "bench", "--manifest-path", str(workspace / "Cargo.toml"),
             "--target-dir", str(CACHE / "build"), "--bench", "matching"]
    env = os.environ.copy()
    env["NCP_BENCH_SIZE"] = args.size
    env["NCP_BENCH_THREADS"] = f"1,{args.threads}"
    run([*cargo, "--workspace", "--no-run"], env)

    # A fresh result directory prevents interrupted runs from reusing stale estimates.
    results = CACHE / "results" / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    results.mkdir(parents=True)
    env["CRITERION_HOME"] = str(results)
    selected = cases(args.size, args.threads)
    pattern = "^(" + "|".join(re.escape(case[2]) for case in selected) + ")$"
    for name in ["original", "current"]:
        run([*cargo, "--locked", "--package", f"compare-{name}", "--", pattern,
             "--noplot", "--save-baseline", name,
             "--sample-size", str(args.sample_size),
             "--warm-up-time", str(args.warm_up_time),
             "--measurement-time", str(args.measurement_time)], env)
    report = markdown(args, (baseline, current), selected,
                      estimates(results, "original", selected),
                      estimates(results, "current", selected))
    (results / "report.md").write_text(report, encoding="utf-8")
    shutil.copy2(workspace / "Cargo.lock", results / "Cargo.lock")
    print(f"Raw results and report: {results}", file=sys.stderr)
    print(report, end="")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
    except KeyboardInterrupt:
        sys.exit("Benchmark comparison interrupted")
