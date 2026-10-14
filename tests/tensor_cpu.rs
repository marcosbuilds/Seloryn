use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn source_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("seloryn-n1-{name}-{}.sely", std::process::id()))
}

fn run_source(name: &str, source: &str) -> Output {
    let path = source_path(name);
    fs::write(&path, source).expect("write N1 source");
    let output = Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(["run", path.to_str().expect("temporary path is UTF-8")])
        .output()
        .expect("start Seloryn");
    let _ = fs::remove_file(path);
    output
}

#[test]
fn runs_f32_matmul_end_to_end_from_crlf_unicode_path() {
    let source = "fn principal() -> Tensor[F32]:\r\n    let esquerda: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\r\n    let direita: Tensor[F32] = tensor_f32([5.0, 6.0, 7.0, 8.0], [2, 2])\r\n    return matmul(esquerda, direita)\r\n\r\nfn main() -> Tensor[F32]:\r\n    return principal()\r\n";
    let path = source_path("resultado-入口");
    fs::write(&path, source).expect("write CRLF Unicode source");
    let check = Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(["check", path.to_str().expect("temporary path is UTF-8")])
        .output()
        .expect("start checker");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let first = Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(["run", path.to_str().expect("temporary path is UTF-8")])
        .output()
        .expect("start interpreter");
    let _ = fs::remove_file(path);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(
        String::from_utf8(first.stdout).expect("tensor result is UTF-8"),
        "Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]\n"
    );
}

#[test]
fn runs_the_public_tensor_example() {
    let output = Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(["run", "examples/tensors/tensor-matmul.sely"])
        .output()
        .expect("start public example");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("example result is UTF-8"),
        "Tensor[F32, shape=[2, 2], values=[19.0, 22.0, 43.0, 50.0]]\n"
    );
}

#[test]
fn runs_broadcast_reshape_and_reduction_end_to_end() {
    let source = "fn main() -> Tensor[F32]:\n    let left: Tensor[F32] = tensor_f32([1.0, 2.0], [2, 1])\n    let right: Tensor[F32] = tensor_f32([10.0, 20.0, 30.0], [1, 3])\n    let broadcasted: Tensor[F32] = left + right\n    let reshaped: Tensor[F32] = reshape(broadcasted, [3, 2])\n    return tensor_sum(reshaped)\n";
    let output = run_source("broadcast", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("tensor result is UTF-8"),
        "Tensor[F32, shape=[], values=[129.0]]\n"
    );

    let source = "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\n    return tensor_mean(values)\n";
    let output = run_source("mean", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("mean result is UTF-8"),
        "Tensor[F32, shape=[], values=[2.5]]\n"
    );

    let source = "fn main() -> F32:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\n    let mean: Tensor[F32] = tensor_mean(values)\n    return tensor_item_f32(mean)\n";
    let output = run_source("mean-item", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"2.5\n");

    let source = "fn main() -> F32:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0], [2])\n    return tensor_item_f32(values)\n";
    let output = run_source("mean-item-invalid", source);
    assert_eq!(output.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&output.stderr).contains("SEL-R205"));
}

#[test]
fn runs_axis_reductions_and_rejects_invalid_axis() {
    let source = "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\n    return tensor_sum(values, 0)\n";
    let output = run_source("axis-sum", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Tensor[F32, shape=[2], values=[4.0, 6.0]]\n"
    );

    let source = "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\n    return tensor_mean(values, 1)\n";
    let output = run_source("axis-mean", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Tensor[F32, shape=[2], values=[1.5, 3.5]]\n"
    );

    let source = "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\n    return tensor_sum(values, 2)\n";
    let first = run_source("axis-invalid", source);
    let second = run_source("axis-invalid", source);
    assert!(!first.status.success());
    assert_eq!(first.status.code(), Some(65));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stderr, second.stderr);
    assert!(String::from_utf8_lossy(&first.stderr).contains("SEL-R205"));
}

#[test]
fn runs_i64_tensor_and_inspection_builtins() {
    let source = "fn main() -> Tensor[I64]:\n    let values: Tensor[I64] = tensor_i64([1, 2, 3, 4], [2, 2])\n    return tensor_sum(values)\n";
    let output = run_source("i64", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("tensor result is UTF-8"),
        "Tensor[I64, shape=[], values=[10]]\n"
    );

    let source = "fn main() -> I64:\n    let values: Tensor[I64] = tensor_i64([1, 2, 3, 4], [2, 2])\n    return tensor_elements(values)\n";
    let output = run_source("elements", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("query result is UTF-8"),
        "4\n"
    );

    let source = "fn main() -> List[I64]:\n    let values: Tensor[I64] = tensor_i64([1, 2, 3, 4], [2, 2])\n    return tensor_shape(values)\n";
    let output = run_source("shape", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("shape result is UTF-8"),
        "[2, 2]\n"
    );

    let source = "fn main() -> I64:\n    let values: Tensor[I64] = tensor_i64([1, 2, 3, 4], [2, 2])\n    return tensor_rank(values)\n";
    let output = run_source("rank", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("rank result is UTF-8"),
        "2\n"
    );

    let source = "fn main() -> Text:\n    let values: Tensor[I64] = tensor_i64([1, 2, 3, 4], [2, 2])\n    return tensor_dtype(values)\n";
    let output = run_source("dtype", source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("dtype result is UTF-8"),
        "\"I64\"\n"
    );
}

#[test]
fn runs_typed_tensor_indexing_and_rejects_invalid_coordinates() {
    let f32_output = run_source(
        "index-f32",
        "fn main() -> F32:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\n    return tensor_get_f32(values, [1, 0])\n",
    );
    assert!(
        f32_output.status.success(),
        "{}",
        String::from_utf8_lossy(&f32_output.stderr)
    );
    assert_eq!(f32_output.stdout, b"3.0\n");

    let transposed_output = run_source(
        "index-transposed-rectangular",
        "fn main() -> F32:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3])\n    return tensor_get_f32(transpose(values), [2, 1])\n",
    );
    assert!(
        transposed_output.status.success(),
        "{}",
        String::from_utf8_lossy(&transposed_output.stderr)
    );
    assert_eq!(transposed_output.stdout, b"6.0\n");

    let i64_output = run_source(
        "index-i64",
        "fn main() -> I64:\n    let values: Tensor[I64] = tensor_i64([7, 8, 9], [3])\n    return tensor_get_i64(values, [2])\n",
    );
    assert!(
        i64_output.status.success(),
        "{}",
        String::from_utf8_lossy(&i64_output.stderr)
    );
    assert_eq!(i64_output.stdout, b"9\n");

    let invalid = run_source(
        "index-invalid",
        "fn main() -> F32:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0], [2])\n    return tensor_get_f32(values, [2])\n",
    );
    assert_eq!(invalid.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("SEL-R205"));

    let static_dtype = run_source(
        "index-static-dtype",
        "fn main() -> F32:\n    let values: Tensor[I64] = tensor_i64([1], [1])\n    return tensor_get_f32(values, [0])\n",
    );
    assert_eq!(static_dtype.status.code(), Some(65));
    assert!(String::from_utf8_lossy(&static_dtype.stderr).contains("SEL-T002"));
}

#[test]
fn runs_explicit_lossless_tensor_conversions() {
    let output = run_source(
        "dtype-conversion",
        "fn main() -> Tensor[F32]:\n    let values: Tensor[I64] = tensor_i64([1, 2, 3, 4, 5, 6], [2, 3])\n    return tensor_to_f32(transpose(values))\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"Tensor[F32, shape=[3, 2], values=[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]]\n"
    );

    let output = run_source(
        "dtype-conversion-reverse",
        "fn main() -> Tensor[I64]:\n    let values: Tensor[F32] = tensor_f32([1.0, -2.0, 3.0], [3])\n    return tensor_to_i64(values)\n",
    );
    assert!(output.status.success());
    assert_eq!(
        output.stdout,
        b"Tensor[I64, shape=[3], values=[1, -2, 3]]\n"
    );
}

#[test]
fn rejects_lossy_tensor_conversions_with_a_structured_error() {
    let integer_loss = run_source(
        "dtype-conversion-loss",
        "fn main() -> Tensor[F32]:\n    return tensor_to_f32(tensor_i64([16777217], [1]))\n",
    );
    assert_eq!(integer_loss.status.code(), Some(65));
    assert!(integer_loss.stdout.is_empty());
    assert!(String::from_utf8_lossy(&integer_loss.stderr).contains("SEL-R212"));

    let fractional = run_source(
        "dtype-conversion-fraction",
        "fn main() -> Tensor[I64]:\n    return tensor_to_i64(tensor_f32([1.5], [1]))\n",
    );
    assert_eq!(fractional.status.code(), Some(65));
    assert!(fractional.stdout.is_empty());
    assert!(String::from_utf8_lossy(&fractional.stderr).contains("SEL-R212"));
}

#[test]
fn rejects_static_dtype_mismatch_before_execution() {
    let source = "fn main() -> Tensor[F32]:\n    let left: Tensor[I64] = tensor_i64([1], [1])\n    let right: Tensor[F32] = tensor_f32([2.0], [1])\n    return left + right\n";
    let output = run_source("static-dtype", source);
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SEL-T003"));
}

#[test]
fn reports_bad_shape_and_matmul_without_panicking() {
    let bad_shape = run_source(
        "bad-shape",
        "fn main() -> Tensor[F32]:\n    return tensor_f32([1.0, 2.0, 3.0], [2, 2])\n",
    );
    assert_eq!(bad_shape.status.code(), Some(65));
    assert!(bad_shape.stdout.is_empty());
    assert!(String::from_utf8_lossy(&bad_shape.stderr).contains("SEL-R203"));

    let bad_matmul = run_source(
        "bad-matmul",
        "fn main() -> Tensor[F32]:\n    let left: Tensor[F32] = tensor_f32([1.0, 2.0], [1, 2])\n    let right: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0], [3, 1])\n    return matmul(left, right)\n",
    );
    assert_eq!(bad_matmul.status.code(), Some(65));
    assert!(bad_matmul.stdout.is_empty());
    assert!(String::from_utf8_lossy(&bad_matmul.stderr).contains("SEL-R205"));
}

#[test]
fn reports_budget_overflow_and_deterministic_runtime_errors() {
    let source = "fn main() -> Tensor[F32]:\n    return tensor_zeros_f32([5000, 5000])\n";
    let first = run_source("budget", source);
    let second = run_source("budget-repeat", source);
    assert_eq!(first.status.code(), Some(65));
    assert_eq!(first.stdout, second.stdout);
    let first_stderr = String::from_utf8(first.stderr).expect("first diagnostic is UTF-8");
    let second_stderr = String::from_utf8(second.stderr).expect("second diagnostic is UTF-8");
    assert_eq!(
        first_stderr,
        second_stderr.replace("budget-repeat", "budget")
    );
    assert!(first_stderr.contains("SEL-R202"));
}

#[test]
fn runs_n2_primitives_end_to_end_with_real_tensors() {
    let transpose = run_source(
        "transpose-entrada-入口",
        "fn main() -> Tensor[F32]:\r\n    return transpose(tensor_f32([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3]))\r\n",
    );
    assert!(
        transpose.status.success(),
        "{}",
        String::from_utf8_lossy(&transpose.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&transpose.stdout),
        "Tensor[F32, shape=[3, 2], values=[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]]\n"
    );

    let unary = run_source(
        "n2-unary",
        "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([-1.0, 0.0, 1.0, 2.0], [2, 2])\n    return tensor_relu(values)\n",
    );
    assert!(
        unary.status.success(),
        "{}",
        String::from_utf8_lossy(&unary.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&unary.stdout),
        "Tensor[F32, shape=[2, 2], values=[0.0, 0.0, 1.0, 2.0]]\n"
    );

    let stable = run_source(
        "n2-stable",
        "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([1000.0, 1001.0, 1002.0, 1003.0], [2, 2])\n    return tensor_logsumexp(values, 1)\n",
    );
    assert!(
        stable.status.success(),
        "{}",
        String::from_utf8_lossy(&stable.stderr)
    );
    let stable_output = String::from_utf8_lossy(&stable.stdout);
    assert!(stable_output.starts_with("Tensor[F32, shape=[2], values=[1001.313"));

    let softmax = run_source(
        "n2-softmax",
        "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])\n    return tensor_softmax(values, 1)\n",
    );
    assert!(
        softmax.status.success(),
        "{}",
        String::from_utf8_lossy(&softmax.stderr)
    );
    assert!(String::from_utf8_lossy(&softmax.stdout).contains("0.26894143"));

    let reduced = run_source(
        "n2-sum-to-shape",
        "fn main() -> Tensor[F32]:\n    let values: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3])\n    return tensor_sum_to_shape(values, [1, 3])\n",
    );
    assert!(
        reduced.status.success(),
        "{}",
        String::from_utf8_lossy(&reduced.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&reduced.stdout),
        "Tensor[F32, shape=[1, 3], values=[5.0, 7.0, 9.0]]\n"
    );
}

#[test]
fn rejects_n2_static_and_runtime_failures_without_execution() {
    let static_error = run_source(
        "n2-static",
        "fn main() -> Tensor[I64]:\n    let values: Tensor[I64] = tensor_i64([1], [1])\n    return tensor_relu(values)\n",
    );
    assert_eq!(static_error.status.code(), Some(65));
    assert!(static_error.stdout.is_empty());
    assert!(String::from_utf8_lossy(&static_error.stderr).contains("SEL-T"));

    let domain = run_source(
        "n2-domain",
        "fn main() -> Tensor[F32]:\n    return tensor_log(tensor_f32([0.0], [1]))\n",
    );
    assert_eq!(domain.status.code(), Some(65));
    assert!(domain.stdout.is_empty());
    assert!(String::from_utf8_lossy(&domain.stderr).contains("SEL-R210"));

    let empty = run_source(
        "n2-empty",
        "fn main() -> Tensor[F32]:\n    return tensor_softmax(tensor_zeros_f32([0, 2]), 0)\n",
    );
    assert_eq!(empty.status.code(), Some(65));
    assert!(empty.stdout.is_empty());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("SEL-R211"));

    let shape = run_source(
        "n2-target-shape",
        "fn main() -> Tensor[F32]:\n    return tensor_sum_to_shape(tensor_zeros_f32([2, 2]), [3])\n",
    );
    assert_eq!(shape.status.code(), Some(65));
    assert!(shape.stdout.is_empty());
    assert!(String::from_utf8_lossy(&shape.stderr).contains("SEL-R204"));
}
