#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path


def take_python_snapshot(demo: str, out_path: Path, columns: int, rows: int) -> None:
    repo_root = Path(__file__).resolve().parents[1]
    textual_repo = repo_root.parent / "textual"
    demo_path = textual_repo / "docs" / "examples" / "widgets" / f"{demo}.py"
    if not demo_path.exists():
        raise FileNotFoundError(f"Python demo not found: {demo_path}")

    sys.path.insert(0, str(textual_repo / "src"))
    from textual._doc import take_svg_screenshot

    svg = take_svg_screenshot(
        app_path=str(demo_path),
        terminal_size=(columns, rows),
        wait_for_animation=False,
        simplify=False,
    )
    out_path.write_text(svg, encoding="utf-8")


def take_rust_snapshot(demo: str, out_path: Path, columns: int, rows: int) -> None:
    repo_root = Path(__file__).resolve().parents[1]
    env = os.environ.copy()
    env.setdefault("CARGO_INCREMENTAL", "0")
    env.setdefault("CARGO_TARGET_DIR", "/tmp/textual-rs-target")
    example_map = {
        "button": "buttons",
    }
    example = example_map.get(demo, demo)
    subprocess.check_call(
        [
            "cargo",
            "run",
            "--quiet",
            "--example",
            example,
            "--",
            "--snapshot",
            str(out_path),
            "--width",
            str(columns),
            "--height",
            str(rows),
        ],
        cwd=repo_root,
        env=env,
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate Python + Rust SVG snapshots for Textual demos."
    )
    parser.add_argument("--demo", default="button", help="Demo name (e.g. button)")
    parser.add_argument("--columns", type=int, default=80, help="Terminal columns")
    parser.add_argument("--rows", type=int, default=24, help="Terminal rows")
    parser.add_argument(
        "--out-dir",
        default=".",
        help="Output directory for demo_python.svg and demo_rust.svg",
    )
    args = parser.parse_args()

    out_dir = Path(args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    python_out = out_dir / f"{args.demo}_python.svg"
    rust_out = out_dir / f"{args.demo}_rust.svg"

    take_python_snapshot(args.demo, python_out, args.columns, args.rows)
    take_rust_snapshot(args.demo, rust_out, args.columns, args.rows)

    print(f"wrote {python_out}")
    print(f"wrote {rust_out}")


if __name__ == "__main__":
    main()
