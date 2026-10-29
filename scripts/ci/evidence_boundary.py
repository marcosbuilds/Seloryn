"""Validate the public evidence/data boundary catalog.

The catalog is deliberately independent from the runtime.  It describes the
small files used by public examples and refuses to treat a fixture as
production data unless the minimum provenance fields are present.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
CATALOG_PATH = ROOT / "docs" / "benchmarks" / "evidence-catalog.json"
ALLOWED_CLASSIFICATIONS = {
    "synthetic-fixture",
    "public-dataset",
    "domain-inspired",
    "production-data",
}
ALLOWED_CLAIM_SCOPES = {"fixture-only", "public-dataset", "domain-study", "production"}
REQUIRED_ENTRY_FIELDS = {
    "id",
    "path",
    "classification",
    "role",
    "source",
    "license",
    "shape",
    "split",
    "target",
    "null_handling",
    "outlier_handling",
    "file_sha256",
    "revision",
    "baseline",
    "validity_limit",
    "public_claim",
    "claims_not_supported",
    "claim_scope",
}
PRODUCTION_PROVENANCE_FIELDS = {
    "source_name",
    "source_url",
    "publisher_or_owner",
    "access_date",
    "license",
    "collection_window",
    "consent_or_legal_basis",
    "transformation_log",
    "dataset_version",
    "independent_verification",
}


class CatalogError(ValueError):
    """Raised when public evidence metadata is incomplete or inaccurate."""


def _require_text(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise CatalogError(f"{label} must be a non-empty string")
    return value


def _require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise CatalogError(f"{label} must be an object")
    return value


def _require_nonempty_list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list) or not value:
        raise CatalogError(f"{label} must be a non-empty list")
    return value


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _contains_null(value: Any) -> bool:
    if value is None:
        return True
    if isinstance(value, dict):
        return any(_contains_null(item) for item in value.values())
    if isinstance(value, list):
        return any(_contains_null(item) for item in value)
    return False


def _safe_repo_path(value: Any, label: str) -> Path:
    relative = Path(_require_text(value, label))
    if relative.is_absolute() or ".." in relative.parts:
        raise CatalogError(f"{label} must be a relative repository path")
    candidate = (ROOT / relative).resolve()
    try:
        candidate.relative_to(ROOT.resolve())
    except ValueError as error:
        raise CatalogError(f"{label} escapes the repository") from error
    return candidate


def _validate_metadata_fields(entry: dict[str, Any]) -> None:
    missing = REQUIRED_ENTRY_FIELDS - set(entry)
    if missing:
        raise CatalogError(f"entry {entry.get('id', '<unknown>')} is missing {sorted(missing)}")
    classification = _require_text(entry["classification"], "classification")
    if classification not in ALLOWED_CLASSIFICATIONS:
        raise CatalogError(f"entry {entry['id']} has unknown classification {classification!r}")
    claim_scope = _require_text(entry["claim_scope"], "claim_scope")
    if claim_scope not in ALLOWED_CLAIM_SCOPES:
        raise CatalogError(f"entry {entry['id']} has unknown claim scope {claim_scope!r}")
    if classification == "production-data" and claim_scope != "production":
        raise CatalogError(f"production-data entry {entry['id']} must use production claim scope")
    if claim_scope == "production" and classification != "production-data":
        raise CatalogError(f"entry {entry['id']} makes a production claim without production-data classification")

    source = _require_mapping(entry["source"], f"entry {entry['id']} source")
    _require_text(source.get("origin"), f"entry {entry['id']} source.origin")
    _require_text(source.get("statement"), f"entry {entry['id']} source.statement")
    license_info = _require_mapping(entry["license"], f"entry {entry['id']} license")
    _require_text(license_info.get("status"), f"entry {entry['id']} license.status")
    _require_text(license_info.get("terms"), f"entry {entry['id']} license.terms")

    shape = _require_mapping(entry["shape"], f"entry {entry['id']} shape")
    _require_text(shape.get("kind"), f"entry {entry['id']} shape.kind")
    rows = shape.get("row_count")
    features = shape.get("feature_count")
    if not isinstance(rows, int) or rows < 1:
        raise CatalogError(f"entry {entry['id']} shape.row_count must be a positive integer")
    if not isinstance(features, int) or features < 0:
        raise CatalogError(f"entry {entry['id']} shape.feature_count must be a non-negative integer")
    feature_names = shape.get("feature_names")
    if not isinstance(feature_names, list) or len(feature_names) != features:
        raise CatalogError(f"entry {entry['id']} feature_names must match feature_count")

    split = _require_mapping(entry["split"], f"entry {entry['id']} split")
    _require_text(split.get("type"), f"entry {entry['id']} split.type")
    _require_text(split.get("strategy"), f"entry {entry['id']} split.strategy")
    _require_text(entry["role"], f"entry {entry['id']} role")
    target = _require_mapping(entry["target"], f"entry {entry['id']} target")
    _require_text(target.get("definition"), f"entry {entry['id']} target.definition")
    nulls = _require_mapping(entry["null_handling"], f"entry {entry['id']} null_handling")
    outliers = _require_mapping(entry["outlier_handling"], f"entry {entry['id']} outlier_handling")
    for name, value in (("null_handling", nulls), ("outlier_handling", outliers)):
        if not isinstance(value.get("observed_count"), int) or value["observed_count"] < 0:
            raise CatalogError(f"entry {entry['id']} {name}.observed_count must be non-negative")
        _require_text(value.get("policy"), f"entry {entry['id']} {name}.policy")
    digest = _require_text(entry["file_sha256"], f"entry {entry['id']} file_sha256").lower()
    if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
        raise CatalogError(f"entry {entry['id']} file_sha256 must be lowercase SHA-256")
    _require_text(entry["revision"], f"entry {entry['id']} revision")
    baseline = _require_mapping(entry["baseline"], f"entry {entry['id']} baseline")
    _require_text(baseline.get("kind"), f"entry {entry['id']} baseline.kind")
    _require_text(baseline.get("description"), f"entry {entry['id']} baseline.description")
    _require_text(entry["validity_limit"], f"entry {entry['id']} validity_limit")
    _require_text(entry["public_claim"], f"entry {entry['id']} public_claim")
    claims_not_supported = _require_nonempty_list(
        entry["claims_not_supported"], f"entry {entry['id']} claims_not_supported"
    )
    if not all(isinstance(item, str) and item.strip() for item in claims_not_supported):
        raise CatalogError(f"entry {entry['id']} claims_not_supported must contain text")

    if classification in {"synthetic-fixture", "domain-inspired"}:
        if source.get("origin") != "synthetic":
            raise CatalogError(f"entry {entry['id']} must explicitly declare synthetic origin")
    if classification == "public-dataset":
        source_url = source.get("source_url")
        if not isinstance(source_url, str) or not source_url.startswith("https://"):
            raise CatalogError(f"public-dataset entry {entry['id']} needs an HTTPS source_url")
    if classification == "production-data" or claim_scope == "production":
        provenance = _require_mapping(entry.get("production_provenance"), f"entry {entry['id']} production_provenance")
        missing_provenance = PRODUCTION_PROVENANCE_FIELDS - set(provenance)
        if missing_provenance:
            raise CatalogError(
                f"entry {entry['id']} production provenance missing {sorted(missing_provenance)}"
            )
        for field in PRODUCTION_PROVENANCE_FIELDS:
            _require_text(provenance.get(field), f"entry {entry['id']} production_provenance.{field}")
        if not provenance["source_url"].startswith("https://"):
            raise CatalogError(f"entry {entry['id']} production source_url must use HTTPS")


def _validate_shape(entry: dict[str, Any], document: Any) -> None:
    shape = entry["shape"]
    kind = shape["kind"]
    label = f"entry {entry['id']}"
    if kind in {"object", "checkpoint"}:
        if not isinstance(document, dict) or shape["row_count"] != 1 or shape["feature_count"] != 0:
            raise CatalogError(f"{label} object shape does not match the file")
        return
    if kind == "parallel-arrays":
        if not isinstance(document, dict):
            raise CatalogError(f"{label} parallel-arrays data must be an object")
        inputs = document.get(shape.get("feature_field", "inputs"))
        targets = document.get(shape.get("target_field", "targets"))
        if not isinstance(inputs, list) or not isinstance(targets, list):
            raise CatalogError(f"{label} parallel-arrays fields are missing")
        if inputs and all(isinstance(row, list) for row in inputs):
            input_shape_valid = len(inputs) == shape["row_count"] and all(
                len(row) == shape["feature_count"] for row in inputs
            )
        else:
            input_shape_valid = len(inputs) == shape["row_count"] * shape["feature_count"]
        if len(targets) != shape["row_count"] or not input_shape_valid:
            raise CatalogError(f"{label} parallel-arrays row or feature count is inaccurate")
        return
    if kind == "numeric-array":
        values = document.get(shape["field"]) if isinstance(document, dict) else None
        if not isinstance(values, list) or len(values) != shape["row_count"] or shape["feature_count"] != 1:
            raise CatalogError(f"{label} numeric-array shape is inaccurate")
        return
    if kind == "matrix-array":
        values = document.get(shape["field"]) if isinstance(document, dict) else None
        if (
            not isinstance(values, list)
            or len(values) != shape["row_count"]
            or any(not isinstance(row, list) or len(row) != shape["feature_count"] for row in values)
        ):
            raise CatalogError(f"{label} matrix-array shape is inaccurate")
        return
    if kind == "token-sequence":
        values = document.get(shape["field"]) if isinstance(document, dict) else None
        marker = shape.get("sequence_start")
        if not isinstance(values, list) or not isinstance(marker, str):
            raise CatalogError(f"{label} token-sequence fields are missing")
        if values.count(marker) != shape["row_count"] or shape["feature_count"] != 1:
            raise CatalogError(f"{label} token-sequence row count is inaccurate")
        return
    if kind == "token-list":
        values = document.get(shape["field"]) if isinstance(document, dict) else None
        if not isinstance(values, list) or len(values) != shape["row_count"] or shape["feature_count"] != 1:
            raise CatalogError(f"{label} token-list shape is inaccurate")
        return
    if kind == "named-records":
        if not isinstance(document, dict):
            raise CatalogError(f"{label} named-records data must be an object")
        record_names = shape.get("record_names")
        feature_names = shape["feature_names"]
        if (
            not isinstance(record_names, list)
            or len(record_names) != shape["row_count"]
            or any(not isinstance(document.get(name), dict) for name in record_names)
            or any(any(field not in document[name] for field in feature_names) for name in record_names)
        ):
            raise CatalogError(f"{label} named-records shape is inaccurate")
        return
    raise CatalogError(f"{label} has unsupported shape kind {kind!r}")


def validate_catalog(catalog: dict[str, Any], root: Path = ROOT) -> list[dict[str, Any]]:
    if catalog.get("schema") != "seloryn.evidence-catalog.v1" or catalog.get("schema_version") != 1:
        raise CatalogError("unsupported evidence catalog schema")
    entries = catalog.get("entries")
    if not isinstance(entries, list) or not entries:
        raise CatalogError("evidence catalog needs entries")

    expected_paths = {
        path.relative_to(root).as_posix()
        for path in (root / "examples").glob("**/data/*.json")
        if path.is_file()
    }
    actual_paths: set[str] = set()
    ids: set[str] = set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise CatalogError("catalog entries must be objects")
        entry_id = _require_text(entry.get("id"), "entry.id")
        if entry_id in ids:
            raise CatalogError(f"duplicate evidence entry id: {entry_id}")
        ids.add(entry_id)
        relative_path = _require_text(entry.get("path"), f"entry {entry_id} path").replace("\\", "/")
        if relative_path in actual_paths:
            raise CatalogError(f"duplicate evidence path: {relative_path}")
        actual_paths.add(relative_path)
        file_path = _safe_repo_path(relative_path, f"entry {entry_id} path")
        if not file_path.is_file():
            raise CatalogError(f"entry {entry_id} points to missing file: {relative_path}")
        _validate_metadata_fields(entry)
        observed_hash = _sha256(file_path)
        if observed_hash != entry["file_sha256"]:
            raise CatalogError(
                f"entry {entry_id} SHA-256 mismatch: metadata={entry['file_sha256']} observed={observed_hash}"
            )
        try:
            document = json.loads(file_path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            raise CatalogError(f"entry {entry_id} is not valid UTF-8 JSON: {error}") from error
        if entry["null_handling"]["observed_count"] == 0 and _contains_null(document):
            raise CatalogError(f"entry {entry_id} declares zero nulls but contains JSON null")
        _validate_shape(entry, document)

    if actual_paths != expected_paths:
        missing = sorted(expected_paths - actual_paths)
        extra = sorted(actual_paths - expected_paths)
        raise CatalogError(f"catalog path coverage mismatch: missing={missing} extra={extra}")
    return entries


def load_catalog(path: Path = CATALOG_PATH) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise CatalogError(f"cannot load evidence catalog {path}: {error}") from error


def main() -> int:
    try:
        catalog = load_catalog()
        entries = validate_catalog(catalog)
    except CatalogError as error:
        print(f"evidence catalog error: {error}")
        return 2
    counts: dict[str, int] = {}
    for entry in entries:
        classification = entry["classification"]
        counts[classification] = counts.get(classification, 0) + 1
    summary = ", ".join(f"{key}={counts[key]}" for key in sorted(counts))
    print(f"Validated {len(entries)} public data entries ({summary})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
