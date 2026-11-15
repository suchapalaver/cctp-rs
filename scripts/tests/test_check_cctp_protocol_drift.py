# SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
#
# SPDX-License-Identifier: Apache-2.0

import copy
import unittest

from scripts.check_cctp_protocol_drift import (
    compare_address_tables,
    parse_address_tables,
)


CONTRACTS = """
## Mainnet contract addresses
### TokenMessengerV2
| Blockchain | Domain | Address |
| - | - | - |
| **Ethereum** | 0 | [`0x1111111111111111111111111111111111111111`](https://example.test) |
### MessageTransmitterV2
| Blockchain | Domain | Address |
| - | - | - |
| **Ethereum** | 0 | `0x2222222222222222222222222222222222222222` |
## Testnet contract addresses
### TokenMessengerV2
| Blockchain | Domain | Address |
| - | - | - |
| **Ethereum Sepolia** | 0 | `0x3333333333333333333333333333333333333333` |
### MessageTransmitterV2
| Blockchain | Domain | Address |
| - | - | - |
| **Ethereum Sepolia** | 0 | `0x4444444444444444444444444444444444444444` |
"""

USDC = """
## Mainnet
| Blockchain | USDC Mainnet Address |
| - | - |
| Ethereum | `0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA` |
## Testnet
| Blockchain | Token Address |
| - | - |
| Ethereum Sepolia | `0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB` |
"""

EURC = """
## Mainnet
| Blockchain Network | Token Address |
| - | - |
| Ethereum | `0xCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC` |
## Testnet
| Blockchain Network | Token Address |
| - | - |
| Ethereum Sepolia | `0xDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD` |
"""


class CctpProtocolDriftTests(unittest.TestCase):
    def test_address_tables_are_canonical_and_strip_markdown(self) -> None:
        tables = parse_address_tables(CONTRACTS, USDC, EURC)

        self.assertEqual(
            tables["cctp_v2_token_messenger_mainnet"],
            [
                {
                    "blockchain": "Ethereum",
                    "domain": 0,
                    "address": "0x1111111111111111111111111111111111111111",
                }
            ],
        )
        self.assertEqual(
            tables["usdc_mainnet"][0]["address"],
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        self.assertEqual(
            tables["eurc_testnet"][0]["address"],
            "0xdddddddddddddddddddddddddddddddddddddddd",
        )

    def test_address_table_mismatch_is_reported(self) -> None:
        published = parse_address_tables(CONTRACTS, USDC, EURC)
        snapshot = copy.deepcopy(published)
        snapshot["usdc_mainnet"][0]["address"] = (
            "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        )
        issues: list[str] = []

        compare_address_tables(published, snapshot, issues)

        self.assertEqual(issues, ["Circle usdc_mainnet address table changed"])


if __name__ == "__main__":
    unittest.main()
