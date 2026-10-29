"""Positive and negative tests for the public evidence boundary catalog."""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "ci"))

from evidence_boundary import CatalogError, load_catalog, validate_catalog  # noqa: E402


class EvidenceBoundaryTests(unittest.TestCase):
    def test_public_catalog_is_complete_and_hash_validated(self) -> None:
        catalog = load_catalog()
        entries = validate_catalog(catalog)

        self.assertEqual(len(entries), 18)
        self.assertEqual(
            {entry["classification"] for entry in entries},
            {"synthetic-fixture", "domain-inspired"},
        )
        self.assertEqual(sum(entry["shape"]["row_count"] for entry in entries), 89)

    def test_production_claim_without_minimum_provenance_is_rejected(self) -> None:
        catalog = copy.deepcopy(load_catalog())
        candidate = catalog["entries"][0]
        candidate["classification"] = "production-data"
        candidate["claim_scope"] = "production"
        candidate.pop("production_provenance", None)

        with self.assertRaisesRegex(CatalogError, "production_provenance"):
            validate_catalog(catalog)

    def test_tampered_fixture_hash_is_rejected(self) -> None:
        catalog = copy.deepcopy(load_catalog())
        catalog["entries"][0]["file_sha256"] = "0" * 64

        with self.assertRaisesRegex(CatalogError, "SHA-256 mismatch"):
            validate_catalog(catalog)

    def test_missing_fixture_entry_is_rejected(self) -> None:
        catalog = copy.deepcopy(load_catalog())
        catalog["entries"].pop()

        with self.assertRaisesRegex(CatalogError, "path coverage mismatch"):
            validate_catalog(catalog)


if __name__ == "__main__":
    unittest.main()
