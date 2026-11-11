# SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
#
# SPDX-License-Identifier: Apache-2.0

import unittest

from scripts.check_nix_flake_freshness import FreshnessError, evaluate_freshness


NOW = 2_000_000_000
DAY = 24 * 60 * 60


def lock_with_timestamps(
    *, nixpkgs: object, rust_overlay: object, transitive: object = 1
) -> dict[str, object]:
    return {
        "root": "root",
        "nodes": {
            "root": {
                "inputs": {
                    "nixpkgs": "nixpkgs",
                    "rust-overlay": "rust-overlay",
                }
            },
            "nixpkgs": {"locked": {"lastModified": nixpkgs}},
            "rust-overlay": {
                "inputs": {"transitive": "transitive"},
                "locked": {"lastModified": rust_overlay},
            },
            "transitive": {"locked": {"lastModified": transitive}},
        },
    }


class NixFlakeFreshnessTests(unittest.TestCase):
    def test_newest_direct_input_keeps_lock_fresh(self) -> None:
        lock = lock_with_timestamps(
            nixpkgs=NOW - 60 * DAY,
            rust_overlay=NOW - DAY,
        )

        result = evaluate_freshness(lock, now_timestamp=NOW, max_age_days=30)

        self.assertFalse(result.is_stale)
        self.assertEqual(result.newest_input, "rust-overlay")

    def test_all_stale_direct_inputs_fail(self) -> None:
        lock = lock_with_timestamps(
            nixpkgs=NOW - 31 * DAY,
            rust_overlay=NOW - 40 * DAY,
            transitive=NOW,
        )

        result = evaluate_freshness(lock, now_timestamp=NOW, max_age_days=30)

        self.assertTrue(result.is_stale)
        self.assertEqual(result.newest_input, "nixpkgs")

    def test_missing_direct_timestamp_is_rejected(self) -> None:
        lock = lock_with_timestamps(nixpkgs=NOW, rust_overlay=None)

        with self.assertRaisesRegex(FreshnessError, "lastModified"):
            evaluate_freshness(lock, now_timestamp=NOW, max_age_days=30)

    def test_future_direct_timestamp_is_rejected(self) -> None:
        lock = lock_with_timestamps(
            nixpkgs=NOW,
            rust_overlay=NOW + 1,
        )

        with self.assertRaisesRegex(FreshnessError, "future"):
            evaluate_freshness(lock, now_timestamp=NOW, max_age_days=30)


if __name__ == "__main__":
    unittest.main()
