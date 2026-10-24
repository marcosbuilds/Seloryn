"""Small dependency-free JSON Schema subset used by the benchmark harness."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any


def _type_matches(value: Any, expected: str | list[str]) -> bool:
    if isinstance(expected, list):
        return any(_type_matches(value, item) for item in expected)
    return {
        "object": isinstance(value, dict),
        "array": isinstance(value, list),
        "string": isinstance(value, str),
        "integer": isinstance(value, int) and not isinstance(value, bool),
        "number": isinstance(value, (int, float)) and not isinstance(value, bool),
        "boolean": isinstance(value, bool),
        "null": value is None,
    }.get(expected, True)


def validate(value: Any, schema: dict[str, Any], path: str = "$", root: dict[str, Any] | None = None) -> list[str]:
    """Return all errors found by the deliberately small schema subset."""

    errors: list[str] = []
    if root is None:
        root = schema
    if "$ref" in schema:
        reference = schema["$ref"]
        if reference.startswith("#/"):
            target: Any = root
            for part in reference[2:].split("/"):
                target = target[part]
            return validate(value, target, path, root)
        errors.append(f"{path}: unsupported external $ref {reference!r}")
        return errors
    if "const" in schema and value != schema["const"]:
        errors.append(f"{path}: expected constant {schema['const']!r}, got {value!r}")
    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{path}: expected one of {schema['enum']!r}, got {value!r}")
    expected = schema.get("type")
    if expected is not None and not _type_matches(value, expected):
        errors.append(f"{path}: expected {expected}, got {type(value).__name__}")
        return errors
    if isinstance(value, str):
        if len(value) < schema.get("minLength", 0):
            errors.append(f"{path}: string is shorter than minLength")
        if "pattern" in schema and re.search(schema["pattern"], value) is None:
            errors.append(f"{path}: string does not match {schema['pattern']!r}")
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        if value < schema.get("minimum", value):
            errors.append(f"{path}: number is below minimum")
        if "exclusiveMinimum" in schema and value <= schema["exclusiveMinimum"]:
            errors.append(f"{path}: number is not above exclusiveMinimum")
    if isinstance(value, list):
        if len(value) < schema.get("minItems", 0):
            errors.append(f"{path}: array has fewer than minItems")
        if "items" in schema:
            for index, item in enumerate(value):
                errors.extend(validate(item, schema["items"], f"{path}[{index}]", root))
    if isinstance(value, dict):
        for required in schema.get("required", []):
            if required not in value:
                errors.append(f"{path}: missing required property {required!r}")
        properties = schema.get("properties", {})
        for key, item in value.items():
            if key in properties:
                errors.extend(validate(item, properties[key], f"{path}.{key}", root))
            elif schema.get("additionalProperties") is False:
                errors.append(f"{path}: unexpected property {key!r}")
    return errors


def load_and_validate(document_path: Path, schema_path: Path) -> Any:
    document = json.loads(document_path.read_text(encoding="utf-8"))
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    errors = validate(document, schema)
    if errors:
        raise ValueError("; ".join(errors))
    return document
