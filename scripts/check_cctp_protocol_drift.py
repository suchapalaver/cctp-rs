#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Joseph Livesey <jlivesey@gmail.com>
#
# SPDX-License-Identifier: Apache-2.0
"""Check Circle's published CCTP protocol surface against local tables."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

DEFAULT_SUPPORTED_PAGE = (
    "https://developers.circle.com/cctp/concepts/"
    "supported-chains-and-domains.md"
)
DEFAULT_SNAPSHOT = Path("scripts/fixtures/circle_cctp_protocol_snapshot.json")
DEFAULT_DOMAIN_RS = Path("src/protocol/domain_id.rs")
DEFAULT_README = Path("README.md")
DEFAULT_AGENTS = Path("AGENTS.md")
TEMP_FAILURE_EXIT = 75
DRIFT_EXIT = 1
CHECK_MARK = "\u2705"
CROSS_MARK = "\u274c"

NAME_ALIASES = {
    "op mainnet": "optimism",
    "optimism": "optimism",
    "polygon pos": "polygon",
    "polygon": "polygon",
    "xdc network": "xdc",
    "xdc": "xdc",
    "starknet testnet": "starknet",
    "starknet": "starknet",
}

EXPECTED_CAPABILITY_COLUMNS = [
    "Blockchain",
    "Source (Standard transfer)",
    "Source (Fast transfer)",
    "Source (Upfront fees)",
    "Forwarding Service",
]


class ParseError(Exception):
    """Raised when Circle's docs no longer match the expected table shape."""


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Compare Circle's published CCTP domain/capability table with "
            "the checked-in drift snapshot, local DomainId enum, and docs counts."
        )
    )
    parser.add_argument(
        "--supported-page",
        default=DEFAULT_SUPPORTED_PAGE,
        help="Circle supported blockchains page URL, or a local Markdown fixture.",
    )
    parser.add_argument(
        "--snapshot",
        type=Path,
        default=DEFAULT_SNAPSHOT,
        help="Checked-in JSON snapshot to compare against.",
    )
    parser.add_argument(
        "--domain-rs",
        type=Path,
        default=DEFAULT_DOMAIN_RS,
        help="Local src/protocol/domain_id.rs path.",
    )
    parser.add_argument(
        "--readme",
        type=Path,
        default=DEFAULT_README,
        help="README path whose domain-count prose should match Circle.",
    )
    parser.add_argument(
        "--agents",
        type=Path,
        default=DEFAULT_AGENTS,
        help="AGENTS.md path whose domain-count prose should match Circle.",
    )
    parser.add_argument(
        "--dump-current",
        type=Path,
        help="Write the parsed current Circle surface as JSON, then continue.",
    )
    args = parser.parse_args()

    try:
        supported_page = read_text_source(args.supported_page)
    except (HTTPError, URLError, TimeoutError, OSError) as exc:
        print(
            "temporary failure: could not fetch Circle's supported "
            f"blockchains page: {exc}",
            file=sys.stderr,
        )
        return TEMP_FAILURE_EXIT

    try:
        published = parse_supported_page(supported_page, args.supported_page)
        snapshot = json.loads(args.snapshot.read_text(encoding="utf-8"))
        local_domains = parse_rust_domain_table(
            args.domain_rs.read_text(encoding="utf-8")
        )
        readme_counts = extract_domain_counts(args.readme.read_text(encoding="utf-8"))
        agents_counts = extract_domain_counts(args.agents.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, ParseError, TypeError, ValueError) as exc:
        print(f"parse failure: {exc}", file=sys.stderr)
        return DRIFT_EXIT

    if args.dump_current:
        args.dump_current.write_text(
            json.dumps(published, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    issues: list[str] = []
    compare_snapshot(published, snapshot, issues)
    compare_local_model(published, local_domains, issues)
    compare_doc_counts("README.md", published, readme_counts, issues)
    compare_doc_counts("AGENTS.md", published, agents_counts, issues)

    if issues:
        print("CCTP protocol drift detected.")
        print(f"Source: {published['source']}")
        print(f"Snapshot: {args.snapshot}")
        for issue in issues:
            print(f"- {issue}")
        print()
        print("Conversion workflow:")
        for step in conversion_workflow():
            print(f"- {step}")
        return DRIFT_EXIT

    print("CCTP protocol drift check passed.")
    print(f"- Source: {published['source']}")
    print(f"- Current CCTP domains: {published['current_cctp_domain_count']}")
    print(f"- V1 legacy-only domains: {len(published['v1_legacy_only_domains'])}")
    print(
        "- Capability rows: "
        f"{len(published['capabilities'])} standard/fast/upfront/forwarding entries"
    )
    return 0


def read_text_source(source: str) -> str:
    if re.match(r"^https?://", source):
        request = Request(
            source,
            headers={
                "Accept": "text/markdown,text/plain;q=0.9,*/*;q=0.1",
                "User-Agent": "cctp-rs-protocol-drift-check",
            },
        )
        with urlopen(request, timeout=30) as response:
            charset = response.headers.get_content_charset() or "utf-8"
            return response.read().decode(charset, errors="replace")

    return Path(source).read_text(encoding="utf-8")


def parse_supported_page(text: str, source: str) -> dict[str, object]:
    capability_columns, capability_rows = parse_markdown_table(
        section(text, "Supported blockchains")
    )
    domain_columns, domain_rows = parse_markdown_table(section(text, "Domain identifiers"))
    v1_columns, v1_rows = parse_markdown_table(section(text, "CCTP V1 (Legacy) only"))

    if capability_columns != EXPECTED_CAPABILITY_COLUMNS:
        raise ParseError(
            "supported blockchains table columns changed: "
            f"{capability_columns!r}"
        )
    if domain_columns != ["Domain", "Blockchain"]:
        raise ParseError(f"domain identifier table columns changed: {domain_columns!r}")
    if v1_columns[:2] != ["Blockchain", "Domain"]:
        raise ParseError(f"V1 legacy table columns changed: {v1_columns!r}")

    domains = [
        {
            "domain": parse_int(row["Domain"], "domain identifier"),
            "blockchain": normalize_cell(row["Blockchain"]),
        }
        for row in domain_rows
    ]
    domains.sort(key=lambda row: row["domain"])

    capabilities = []
    for row in capability_rows:
        capabilities.append(
            {
                "blockchain": public_blockchain_name(row["Blockchain"]),
                "standard_transfer": parse_capability_value(
                    row["Source (Standard transfer)"]
                ),
                "fast_transfer": parse_capability_value(
                    row["Source (Fast transfer)"]
                ),
                "upfront_fees": parse_capability_value(row["Source (Upfront fees)"]),
                "forwarding_service": parse_capability_value(
                    row["Forwarding Service"]
                ),
            }
        )
    capabilities.sort(key=lambda row: name_key(row["blockchain"]))

    v1_legacy_only_domains = [
        {
            "domain": parse_int(row["Domain"], "V1 legacy domain"),
            "blockchain": normalize_cell(row["Blockchain"]),
        }
        for row in v1_rows
    ]
    v1_legacy_only_domains.sort(key=lambda row: row["domain"])

    return {
        "source": source,
        "current_cctp_domain_count": len(domains),
        "capability_columns": capability_columns,
        "domains": domains,
        "capabilities": capabilities,
        "tokens": parse_supported_tokens(section(text, "Supported tokens")),
        "v1_legacy_only_domains": v1_legacy_only_domains,
    }


def section(text: str, heading: str) -> str:
    match = re.search(rf"^## {re.escape(heading)}\s*$", text, flags=re.MULTILINE)
    if not match:
        raise ParseError(f"missing section: {heading}")
    tail = text[match.end() :]
    next_heading = re.search(r"^## ", tail, flags=re.MULTILINE)
    if next_heading:
        return tail[: next_heading.start()]
    return tail


def parse_markdown_table(text: str) -> tuple[list[str], list[dict[str, str]]]:
    lines = [line.strip() for line in text.splitlines() if line.strip().startswith("|")]
    if not lines:
        raise ParseError("missing Markdown table")

    rows = []
    for line in lines:
        cells = [normalize_cell(cell) for cell in line.strip("|").split("|")]
        if all(re.fullmatch(r":?-{2,}:?", cell.replace(" ", "")) for cell in cells):
            continue
        rows.append(cells)

    if len(rows) < 2:
        raise ParseError("Markdown table has no data rows")

    header = rows[0]
    data = []
    for row in rows[1:]:
        if len(row) != len(header):
            raise ParseError(f"table row length changed: {row!r}")
        data.append(dict(zip(header, row, strict=True)))
    return header, data


def normalize_cell(value: str) -> str:
    value = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", value)
    value = value.replace("`", "")
    return re.sub(r"\s+", " ", value).strip()


def parse_int(value: str, label: str) -> int:
    try:
        return int(value)
    except ValueError as exc:
        raise ParseError(f"invalid {label}: {value!r}") from exc


def parse_capability_value(value: str) -> bool | None:
    value = normalize_cell(value)
    if value == CHECK_MARK:
        return True
    if value == CROSS_MARK:
        return False
    if value.upper() in {"N/A", "NA"}:
        return None
    raise ParseError(f"unknown capability value: {value!r}")


def public_blockchain_name(value: str) -> str:
    return re.sub(r"\s*\([^)]*\)", "", normalize_cell(value)).strip()


def parse_supported_tokens(text: str) -> dict[str, object]:
    bullets = re.findall(r"(?ms)^\* (.*?)(?=^\* |\Z)", text)
    tokens: dict[str, object] = {}
    for bullet in bullets:
        bullet = normalize_cell(re.sub(r"\s+", " ", bullet))
        token_match = re.match(r"([A-Z0-9]+):\s*(.*)$", bullet)
        if not token_match:
            continue
        token = token_match.group(1).lower()
        description = token_match.group(2).rstrip(".")
        if "except" in description:
            exceptions = description.split("except", 1)[1]
            tokens[token] = {
                "availability": "all_except",
                "exceptions": split_human_list(exceptions),
            }
        elif "only on" in description:
            supported = description.split("only on", 1)[1]
            tokens[token] = {
                "availability": "only",
                "blockchains": split_human_list(supported),
            }
        else:
            tokens[token] = {"availability": description}

    if "usdc" not in tokens or "usyc" not in tokens:
        raise ParseError(f"token support bullets changed: {tokens!r}")
    return tokens


def split_human_list(value: str) -> list[str]:
    value = value.strip().rstrip(".")
    value = re.sub(r"\s+and\s+", ", ", value)
    return [public_blockchain_name(item) for item in value.split(",") if item.strip()]


def parse_rust_domain_table(text: str) -> list[dict[str, object]]:
    enum_match = re.search(r"pub enum DomainId\s*\{(?P<body>.*?)\n\}", text, re.S)
    if not enum_match:
        raise ParseError("could not locate DomainId enum")
    enum_values = {
        match.group(1): int(match.group(2))
        for match in re.finditer(
            r"^\s*([A-Za-z][A-Za-z0-9_]*)\s*=\s*(\d+),",
            enum_match.group("body"),
            flags=re.MULTILINE,
        )
    }

    name_match = re.search(
        r"pub const fn name\(self\).*?match self\s*\{(?P<body>.*?)\n\s*\}",
        text,
        re.S,
    )
    if not name_match:
        raise ParseError("could not locate DomainId::name match")
    enum_names = {
        match.group(1): match.group(2)
        for match in re.finditer(
            r'Self::([A-Za-z][A-Za-z0-9_]*)\s*=>\s*"([^"]+)"',
            name_match.group("body"),
        )
    }

    missing_names = sorted(set(enum_values) - set(enum_names))
    if missing_names:
        raise ParseError(f"DomainId variants missing name() entries: {missing_names}")

    rows = [
        {"domain": domain, "blockchain": enum_names[variant], "variant": variant}
        for variant, domain in enum_values.items()
    ]
    rows.sort(key=lambda row: row["domain"])
    return rows


def extract_domain_counts(text: str) -> list[int]:
    patterns = [
        r"\ball\s+(\d+)\s+domain IDs\b",
        r"\bdocs list\s+(\d+)\s+current CCTP domain IDs\b",
        r"\b(\d+)-domain CCTP table\b",
    ]
    counts = set()
    for pattern in patterns:
        for match in re.finditer(pattern, text, flags=re.IGNORECASE):
            counts.add(int(match.group(1)))
    return sorted(counts)


def compare_snapshot(
    published: dict[str, object],
    snapshot: dict[str, object],
    issues: list[str],
) -> None:
    if published["current_cctp_domain_count"] != snapshot["current_cctp_domain_count"]:
        issues.append(
            "Circle current CCTP domain count changed from "
            f"{snapshot['current_cctp_domain_count']} to "
            f"{published['current_cctp_domain_count']}"
        )

    compare_domain_rows(
        "current CCTP domain table",
        published["domains"],
        snapshot["domains"],
        issues,
    )
    compare_domain_rows(
        "V1 legacy-only domain table",
        published["v1_legacy_only_domains"],
        snapshot["v1_legacy_only_domains"],
        issues,
    )
    compare_capabilities(
        published["capabilities"],
        snapshot["capabilities"],
        issues,
    )

    if published["capability_columns"] != snapshot["capability_columns"]:
        issues.append(
            "Circle capability columns changed from "
            f"{snapshot['capability_columns']} to {published['capability_columns']}"
        )
    if published["tokens"] != snapshot["tokens"]:
        issues.append(
            f"Circle token availability changed from {snapshot['tokens']} "
            f"to {published['tokens']}"
        )


def compare_domain_rows(
    label: str,
    published_rows: object,
    snapshot_rows: object,
    issues: list[str],
) -> None:
    published = domain_map(published_rows)
    snapshot = domain_map(snapshot_rows)
    for domain in sorted(set(published) - set(snapshot)):
        issues.append(f"new {label} entry: {published[domain]} ({domain})")
    for domain in sorted(set(snapshot) - set(published)):
        issues.append(f"removed {label} entry: {snapshot[domain]} ({domain})")
    for domain in sorted(set(published) & set(snapshot)):
        if published[domain] != snapshot[domain]:
            issues.append(
                f"renamed {label} entry {domain}: "
                f"{snapshot[domain]} -> {published[domain]}"
            )


def compare_capabilities(
    published_rows: object,
    snapshot_rows: object,
    issues: list[str],
) -> None:
    published = capability_map(published_rows)
    snapshot = capability_map(snapshot_rows)
    for key in sorted(set(published) - set(snapshot)):
        issues.append(f"new capability row: {published[key]['blockchain']}")
    for key in sorted(set(snapshot) - set(published)):
        issues.append(f"removed capability row: {snapshot[key]['blockchain']}")
    for key in sorted(set(published) & set(snapshot)):
        for field in [
            "standard_transfer",
            "fast_transfer",
            "upfront_fees",
            "forwarding_service",
        ]:
            if published[key][field] != snapshot[key][field]:
                issues.append(
                    f"{published[key]['blockchain']} {field} changed from "
                    f"{capability_label(snapshot[key][field])} to "
                    f"{capability_label(published[key][field])}"
                )


def compare_local_model(
    published: dict[str, object],
    local_domains: list[dict[str, object]],
    issues: list[str],
) -> None:
    published_by_id = domain_map(published["domains"])
    local_by_id = domain_map(local_domains)

    for domain in sorted(set(published_by_id) - set(local_by_id)):
        issues.append(
            "local DomainId is missing Circle current domain "
            f"{published_by_id[domain]} ({domain})"
        )
    for domain in sorted(set(local_by_id) - set(published_by_id)):
        issues.append(
            "local DomainId includes a domain no longer in Circle's current table: "
            f"{local_by_id[domain]} ({domain})"
        )
    for domain in sorted(set(published_by_id) & set(local_by_id)):
        if name_key(published_by_id[domain]) != name_key(local_by_id[domain]):
            issues.append(
                "local DomainId name no longer matches Circle's current table for "
                f"{domain}: {local_by_id[domain]} vs {published_by_id[domain]}"
            )


def compare_doc_counts(
    label: str,
    published: dict[str, object],
    counts: list[int],
    issues: list[str],
) -> None:
    expected = published["current_cctp_domain_count"]
    if not counts:
        issues.append(f"{label} has no parseable current-domain count")
    elif counts != [expected]:
        issues.append(
            f"{label} current-domain count mentions {counts}, "
            f"but Circle currently lists {expected}"
        )


def domain_map(rows: object) -> dict[int, str]:
    return {
        int(row["domain"]): str(row["blockchain"])
        for row in cast_rows(rows)
    }


def capability_map(rows: object) -> dict[str, dict[str, object]]:
    return {
        name_key(row["blockchain"]): row
        for row in cast_rows(rows)
    }


def cast_rows(rows: object) -> list[dict[str, object]]:
    if not isinstance(rows, list):
        raise TypeError(f"expected list rows, got {type(rows)!r}")
    return rows


def name_key(value: object) -> str:
    name = public_blockchain_name(str(value))
    key = re.sub(r"[^a-z0-9]+", " ", name.lower()).strip()
    return NAME_ALIASES.get(key, key)


def capability_label(value: object) -> str:
    if value is True:
        return "supported"
    if value is False:
        return "not supported"
    if value is None:
        return "not applicable"
    return str(value)


def conversion_workflow() -> list[str]:
    return [
        "New current domain: file/attach an issue, then decide parser-only vs "
        "bridge-supported before changing the public route surface.",
        "Parser-only domain: update DomainId, Lean Domain.lean, generated Lean "
        "fixtures, README/AGENTS counts, and parser tests.",
        "Bridge-supported EVM route: add addresses, chain config, confirmation "
        "times, route docs, and address/domain validation evidence.",
        "Asset or token drift: update asset modeling and Iris endpoint support "
        "before advertising burn or fee support.",
        "Fast/Standard/Forwarding drift: update capability gates and docs before "
        "route builders accept the changed behavior.",
        "V1 legacy-only drift: keep legacy-only domains out of DomainId unless "
        "Circle promotes them into the current CCTP table.",
    ]


if __name__ == "__main__":
    sys.exit(main())
