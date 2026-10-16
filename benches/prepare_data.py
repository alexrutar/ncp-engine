#!/usr/bin/env python3
"""Download, verify, and deterministically sample the matching benchmark inputs.

Requires Python 3.9+ and curl. Run explicitly; cargo bench never downloads data.
"""

import argparse
import gzip
import hashlib
import heapq
import json
from pathlib import Path
import subprocess
import sys

DATA = Path(__file__).resolve().parent / "data"
SEED = b"ncp-matching-v1\0"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify(path, expected):
    actual = sha256(path)
    if actual != expected:
        raise ValueError(f"SHA-256 mismatch for {path}: expected {expected}, got {actual}")


def fetch(source, offline):
    path = DATA / "cache" / source["cache"]
    if path.exists():
        verify(path, source["sha256"])
        return path
    if offline:
        raise FileNotFoundError(f"Missing cached source: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".part")
    print(f"Downloading {source['name']} from {source['url']}", flush=True)
    try:
        subprocess.run(
            ["curl", "--fail", "--location", "--retry", "2", "--connect-timeout", "30",
             "--max-time", "600", "--output", str(temporary), source["url"]],
            check=True,
        )
        verify(temporary, source["sha256"])
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)
    return path


def lines(source, path):
    wikipedia = source["format"] == "wikipedia-titles-gzip"
    opener = gzip.open if wikipedia else open
    with opener(path, "rt", encoding="utf-8") as stream:
        if wikipedia and next(stream).rstrip("\r\n") != "page_title":
            raise ValueError(f"Unexpected Wikipedia title header in {path}")
        for line in stream:
            text = line.rstrip("\r\n")
            if wikipedia:
                text = text.replace("_", " ")
            elif source["format"] == "chromium-paths" and not text.startswith("/chromium/"):
                # Frizbee labels the upstream fixture Chromium, but it includes
                # several other repositories. Keep the agreed Chromium corpus.
                continue
            if "\0" in text:
                raise ValueError(f"Unexpected NUL in {path}")
            if text:
                yield text


def sample_lines(candidates, size):
    # A bounded max heap retains the smallest hashes without loading an entire dump.
    # Sorting the retained entries makes prefixes deterministic nested samples.
    heap = []
    for text in candidates:
        key = int.from_bytes(hashlib.sha256(SEED + text.encode("utf-8")).digest(), "big")
        entry = (-key, text)
        if len(heap) < size:
            heapq.heappush(heap, entry)
        elif entry > heap[0]:
            heapq.heapreplace(heap, entry)
    if len(heap) != size:
        raise ValueError(f"Only {len(heap)} candidates; need {size}")
    return [text for _, text in sorted(heap, key=lambda entry: (-entry[0], entry[1]))]


def write_lines(filename, candidates):
    path = DATA / "prepared" / filename
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".part")
    count = 0
    digest = hashlib.sha256()
    try:
        with temporary.open("wb") as stream:
            for text in candidates:
                data = (text + "\n").encode("utf-8")
                stream.write(data)
                digest.update(data)
                count += 1
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)
    return {"file": filename, "rows": count, "sha256": digest.hexdigest()}


def prepare(source, path, sample_size):
    outputs = [write_lines(
        source["name"] + ".txt", sample_lines(lines(source, path), sample_size)
    )]
    if source["name"] == "chromium":
        outputs.append(write_lines("chromium-full.txt", lines(source, path)))
    return outputs


def main():
    manifest = json.loads((DATA / "manifest.json").read_text(encoding="utf-8"))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true", help="Use only cached downloads")
    parser.add_argument("--verify", action="store_true", help="Verify prepared files without downloading or preparing")
    parser.add_argument("datasets", nargs="*", help="Dataset names; defaults to all four")
    args = parser.parse_args()
    known = {source["name"] for source in manifest["datasets"]}
    if set(args.datasets) - known:
        parser.error(f"Dataset names must be drawn from: {', '.join(sorted(known))}")
    for source in manifest["datasets"]:
        if args.datasets and source["name"] not in args.datasets:
            continue
        if args.verify:
            for output in source["outputs"]:
                verify(DATA / "prepared" / output["file"], output["sha256"])
        else:
            outputs = prepare(source, fetch(source, args.offline), manifest["sample_size"])
            if outputs != source["outputs"]:
                raise ValueError(f"Prepared data differs from the pinned manifest: {source['name']}")
        print(f"Verified {source['name']}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
