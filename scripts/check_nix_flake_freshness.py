#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
#
# SPDX-License-Identifier: Apache-2.0
"""Check that the committed Nix inputs were refreshed recently."""

from __future__ import annotations

import argparse
import json
import sys
import time
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path


DEFAULT_LOCKFILE = Path("flake.lock")
DEFAULT_MAX_AGE_DAYS = 30
SECONDS_PER_DAY = 24 * 60 * 60


class FreshnessError(Exception):
    """Raised when flake.lock does not expose trustworthy freshness data."""


@dataclass(frozen=True)
class FreshnessResult:
    """Freshness of the newest direct input in the committed lockfile."""

    newest_input: str
    newest_timestamp: int
    age_seconds: int
    max_age_seconds: int

    @property
    def is_stale(self) -> bool:
        """Return whether the newest direct input exceeds the age budget."""

        return self.age_seconds > self.max_age_seconds


def direct_input_timestamps(lock: object) -> dict[str, int]:
    """Return last-modified timestamps for the root flake's direct inputs."""

    if not isinstance(lock, dict):
        raise FreshnessError("flake.lock root must be a JSON object")

    nodes = lock.get("nodes")
    root_name = lock.get("root")
    if not isinstance(nodes, dict) or not isinstance(root_name, str):
        raise FreshnessError("flake.lock is missing nodes or root metadata")

    root = nodes.get(root_name)
    if not isinstance(root, dict) or not isinstance(root.get("inputs"), dict):
        raise FreshnessError("flake.lock root node is missing direct inputs")

    timestamps: dict[str, int] = {}
    for input_name, node_name in sorted(root["inputs"].items()):
        if not isinstance(input_name, str) or not isinstance(node_name, str):
            raise FreshnessError(f"direct input {input_name!r} has an invalid node")
        node = nodes.get(node_name)
        locked = node.get("locked") if isinstance(node, dict) else None
        timestamp = locked.get("lastModified") if isinstance(locked, dict) else None
        if not isinstance(timestamp, int) or isinstance(timestamp, bool):
            raise FreshnessError(
                f"direct input {input_name!r} is missing an integer lastModified"
            )
        timestamps[input_name] = timestamp

    if not timestamps:
        raise FreshnessError("flake.lock root has no direct inputs")
    return timestamps


def evaluate_freshness(
    lock: object, *, now_timestamp: int, max_age_days: int
) -> FreshnessResult:
    """Evaluate the newest direct input against a maximum refresh age."""

    if max_age_days <= 0:
        raise FreshnessError("max age must be greater than zero days")

    timestamps = direct_input_timestamps(lock)
    future_inputs = sorted(
        name for name, value in timestamps.items() if value > now_timestamp
    )
    if future_inputs:
        raise FreshnessError(
            "direct input timestamps are in the future: " + ", ".join(future_inputs)
        )

    newest_input, newest_timestamp = max(timestamps.items(), key=lambda item: item[1])
    return FreshnessResult(
        newest_input=newest_input,
        newest_timestamp=newest_timestamp,
        age_seconds=now_timestamp - newest_timestamp,
        max_age_seconds=max_age_days * SECONDS_PER_DAY,
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fail when the committed direct Nix inputs are too stale."
    )
    parser.add_argument("--lockfile", type=Path, default=DEFAULT_LOCKFILE)
    parser.add_argument("--max-age-days", type=int, default=DEFAULT_MAX_AGE_DAYS)
    parser.add_argument(
        "--now-timestamp",
        type=int,
        default=None,
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args()

    try:
        lock = json.loads(args.lockfile.read_text(encoding="utf-8"))
        now_timestamp = (
            args.now_timestamp if args.now_timestamp is not None else int(time.time())
        )
        result = evaluate_freshness(
            lock,
            now_timestamp=now_timestamp,
            max_age_days=args.max_age_days,
        )
    except (OSError, json.JSONDecodeError, FreshnessError) as exc:
        print(f"Nix flake freshness check failed: {exc}", file=sys.stderr)
        return 1

    updated_at = datetime.fromtimestamp(result.newest_timestamp, UTC).isoformat()
    age_days = result.age_seconds // SECONDS_PER_DAY
    if result.is_stale:
        print(
            "Nix flake inputs are stale: newest direct input "
            f"{result.newest_input!r} is {age_days} days old "
            f"({updated_at}); maximum is {args.max_age_days} days.",
            file=sys.stderr,
        )
        return 1

    print(
        "Nix flake freshness check passed: newest direct input "
        f"{result.newest_input!r} is {age_days} days old ({updated_at})."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
