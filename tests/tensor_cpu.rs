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
        .args(["run", "examples/tensor-matmul.sely"])
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
