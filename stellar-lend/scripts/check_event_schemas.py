#!/usr/bin/env python3
"""Validate StellarLend Soroban event schema conventions.

The checker is intentionally source-based so it runs quickly in CI and catches drift before
generated contract specs or indexers fall out of sync.
"""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONTRACTS = ROOT / "contracts"
SCHEMA_PATH = ROOT / "docs" / "event-schema.v1.json"
DOC_PATH = ROOT / "docs" / "event-schema.md"
COMMON_SCHEMA_PATH = ROOT / "contracts" / "common" / "src" / "event_schema.rs"
INDEXER_SCHEMA_PATH = ROOT / "indexing_system" / "src" / "schema.rs"

EVENT_STRUCT_RE = re.compile(
    r"(?P<attrs>(?:#\[[^\]]+\]\s*)*)pub\s+struct\s+(?P<name>[A-Za-z0-9_]+)\s*\{(?P<body>.*?)\n\}",
    re.DOTALL,
)
FIELD_RE = re.compile(r"pub\s+(?P<name>[a-zA-Z0-9_]+)\s*:\s*(?P<ty>[^,]+),")
TOPICS_RE = re.compile(r"topics\s*=\s*\[(?P<topics>[^\]]*)\]")
STRING_RE = re.compile(r'"([^"]+)"')
PUBLISH_RE = re.compile(r"env\.events\(\)\s*\.publish\s*\((?P<args>.*?)\);", re.DOTALL)


@dataclass(frozen=True)
class EventDef:
    contract: str
    path: Path
    name: str
    topic: str
    fields: tuple[str, ...]
    topic_field_count: int
    static_topic_count: int


def snake_case(name: str) -> str:
    chars: list[str] = []
    previous_lower_or_digit = False
    for ch in name:
        if ch in {"-", " "}:
            chars.append("_")
            previous_lower_or_digit = False
            continue
        if ch.isupper():
            if previous_lower_or_digit:
                chars.append("_")
            chars.append(ch.lower())
            previous_lower_or_digit = False
        else:
            chars.append(ch)
            previous_lower_or_digit = ch.islower() or ch.isdigit()
    return "".join(chars)


def contract_name(path: Path) -> str:
    rel = path.relative_to(CONTRACTS)
    return rel.parts[0]


def is_lower_snake(value: str) -> bool:
    return bool(re.fullmatch(r"[a-z][a-z0-9_]*", value))


def explicit_topics(attrs: str) -> list[str]:
    match = TOPICS_RE.search(attrs)
    if not match:
        return []
    return STRING_RE.findall(match.group("topics"))


def parse_event_structs() -> list[EventDef]:
    events: list[EventDef] = []
    for path in sorted(CONTRACTS.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        for match in EVENT_STRUCT_RE.finditer(text):
            attrs = match.group("attrs")
            if "contractevent" not in attrs:
                continue
            name = match.group("name")
            body = match.group("body")
            topics = explicit_topics(attrs)
            fields = tuple(field.group("name") for field in FIELD_RE.finditer(body))
            topic_fields = body.count("#[topic]")
            topic = topics[0] if topics else snake_case(name)
            events.append(
                EventDef(
                    contract=contract_name(path),
                    path=path,
                    name=name,
                    topic=topic,
                    fields=fields,
                    topic_field_count=topic_fields,
                    static_topic_count=len(topics),
                )
            )
    return events


def parse_manual_topics() -> set[tuple[str, str]]:
    topics: set[tuple[str, str]] = set()
    for path in sorted(CONTRACTS.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if "env.events().publish" not in text:
            continue
        for match in PUBLISH_RE.finditer(text):
            strings = STRING_RE.findall(match.group("args"))
            if strings:
                topics.add((contract_name(path), strings[0]))
    return topics


def load_schema() -> dict:
    return json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))


def check_constants(schema_version: int, errors: list[str]) -> None:
    common = COMMON_SCHEMA_PATH.read_text(encoding="utf-8")
    indexer = INDEXER_SCHEMA_PATH.read_text(encoding="utf-8")
    expected_common = f"EVENT_SCHEMA_VERSION: u32 = {schema_version}"
    expected_indexer = f"STANDARD_EVENT_SCHEMA_VERSION: u32 = {schema_version}"
    if expected_common not in common:
        errors.append(f"{COMMON_SCHEMA_PATH}: missing `{expected_common}`")
    if expected_indexer not in indexer:
        errors.append(f"{INDEXER_SCHEMA_PATH}: missing `{expected_indexer}`")


def main() -> int:
    schema = load_schema()
    schema_version = int(schema["schema_version"])
    allowed_contracts = {entry["name"] for entry in schema["contracts"] if entry.get("active")}
    allowed_overloads = {
        (entry["contract"], entry["topic"]) for entry in schema.get("allow_overloaded_topics", [])
    }
    legacy_manual = {
        (entry["contract"], entry["topic"]) for entry in schema.get("legacy_manual_events", [])
    }

    errors: list[str] = []
    check_constants(schema_version, errors)

    if f"Schema Version: {schema_version}" not in DOC_PATH.read_text(encoding="utf-8"):
        errors.append(f"{DOC_PATH}: missing schema version heading")

    events = parse_event_structs()
    if not events:
        errors.append("No #[contractevent] structs found")

    seen_by_contract_topic: dict[tuple[str, str], EventDef] = {}
    for event in events:
        rel = event.path.relative_to(ROOT)
        if event.contract not in allowed_contracts:
            errors.append(f"{rel}: contract `{event.contract}` is not listed in schema JSON")
        if not event.name.endswith("Event"):
            errors.append(f"{rel}: event struct `{event.name}` must end with `Event`")
        if not is_lower_snake(event.topic):
            errors.append(f"{rel}: topic `{event.topic}` must be lower_snake_case")
        if event.static_topic_count + event.topic_field_count > 4:
            errors.append(
                f"{rel}: `{event.name}` uses {event.static_topic_count + event.topic_field_count} topics"
            )
        for field in event.fields:
            if not is_lower_snake(field):
                errors.append(f"{rel}: field `{event.name}.{field}` must be lower_snake_case")

        key = (event.contract, event.topic)
        previous = seen_by_contract_topic.get(key)
        if previous and previous.fields != event.fields and key not in allowed_overloads:
            errors.append(
                f"{rel}: topic `{event.topic}` is overloaded by `{previous.name}` and `{event.name}`"
            )
        seen_by_contract_topic[key] = event

    manual_topics = parse_manual_topics()
    for contract, topic in sorted(manual_topics):
        if (contract, topic) not in legacy_manual:
            errors.append(
                f"contracts/{contract}: manual event topic `{topic}` must be listed in legacy_manual_events"
            )

    if errors:
        print("Event schema compliance failed:")
        for error in errors:
            print(f"  - {error}")
        return 1

    print(
        f"Event schema v{schema_version} OK: {len(events)} typed events, "
        f"{len(manual_topics)} legacy manual topics"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
