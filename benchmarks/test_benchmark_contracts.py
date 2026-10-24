"""Fast stdlib checks for the benchmark manifest and independent oracles."""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from runner import MANIFEST_PATH, MANIFEST_SCHEMA, ROOT, case_limitations, import_reference, parse_tensor
from schema_validator import load_and_validate


class BenchmarkContractTests(unittest.TestCase):
    def test_manifest_matches_schema_and_declares_all_cases(self) -> None:
        manifest = load_and_validate(MANIFEST_PATH, MANIFEST_SCHEMA)
        self.assertEqual([case["id"] for case in manifest["cases"]], [f"BENCH-0{index}" for index in range(1, 7)])

    def test_references_are_stdlib_and_return_success_or_expected_error(self) -> None:
        manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
        for case in manifest["cases"]:
            _, reference = import_reference(ROOT / case["reference"])
            result = reference(ROOT)
            self.assertEqual(result["status"], "error" if case["id"] == "BENCH-06" else "ok")

    def test_bench04_reference_preserves_tensor_f32_rounding(self) -> None:
        _, reference = import_reference(
            ROOT / "benchmarks" / "references" / "bench_04_small_neural_network.py"
        )
        result = reference(ROOT)
        expected = {
            "initial_loss": 9.014183044433594,
            "final_loss": 0.018084624782204628,
            "evaluation_loss": 0.15925757586956024,
            "reloaded_loss": 0.15925757586956024,
            "prediction": -1.3103904724121094,
        }
        for field, value in expected.items():
            self.assertEqual(result[field], value, field)

    def test_bench04_failure_limitation_is_not_reported_after_a_pass(self) -> None:
        message = "The current run records a failed independent numerical-equivalence check"
        self.assertFalse(any(message in item for item in case_limitations("BENCH-04", status="passed")))
        self.assertTrue(any(message in item for item in case_limitations("BENCH-04", status="failed")))

    def test_tensor_parser_keeps_shape_and_values(self) -> None:
        parsed = parse_tensor("Tensor[F32, shape=[2, 2], values=[1.0, 2.0, 3.0, 4.0]]\n")
        self.assertEqual(parsed["shape"], [2, 2])
        self.assertEqual(parsed["values"], [1.0, 2.0, 3.0, 4.0])


if __name__ == "__main__":
    unittest.main()
