"""Check local Markdown links and public documentation path contracts."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[2]
LINK = re.compile(r"\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
OBSOLETE_SOURCE_LINKS = (
    "src/main.rs",
    "src/runtime.rs",
    "src/job.rs",
    "src/job_worker.rs",
    "src/semantic.rs",
    "src/parser.rs",
    "src/ir.rs",
    "src/lexer.rs",
    "src/analysis.rs",
    "src/lsp.rs",
    "src/gpu.rs",
    "src/autograd.rs",
    "src/tensor_autograd.rs",
    "src/project.rs",
    "src/workspace.rs",
    "src/tensor/",
)


def main() -> int:
    failures: list[str] = []
    for document in ROOT.rglob("*.md"):
        if any(part in {".git", "target", "node_modules"} for part in document.parts):
            continue
        text = document.read_text(encoding="utf-8")
        for match in LINK.finditer(text):
            target = unquote(match.group(1))
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            target_path = target.split("#", 1)[0]
            if not target_path:
                continue
            resolved = (document.parent / target_path).resolve()
            try:
                resolved.relative_to(ROOT.resolve())
            except ValueError:
                failures.append(f"{document.relative_to(ROOT)}: link escapes repository: {target}")
                continue
            if not resolved.exists():
                failures.append(f"{document.relative_to(ROOT)}: missing local link: {target}")
        if document.name in {"README.md", "install.md", "editor.md", "downloads.md"}:
            if re.search(r"\]\([^)]*/releases/latest[^)]*\)", text):
                failures.append(f"{document.relative_to(ROOT)}: latest-release alias is not a verified source")
        for obsolete in OBSOLETE_SOURCE_LINKS:
            if obsolete in text and "src/main.sely" not in obsolete:
                failures.append(f"{document.relative_to(ROOT)}: stale source path: {obsolete}")
    if failures:
        print("PUBLIC DOCUMENTATION FAILED", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("PUBLIC DOCUMENTATION PASS: local Markdown links and source paths")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
