"""Python side of the Py-vs-Rust parity comparison.

Usage:
    venv\\Scripts\\python.exe scripts\\parity_py_side.py <file.xlsx> <outdir> <tolerance> <target1,target2,...>

Prints elapsed time and the produced output path (last line = OUTPUT:<path>).
"""

import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "backend-py"))

from app.invoice_finder import find_invoice_combinations_for_targets


def main() -> None:
    xlsx_path = Path(sys.argv[1]).resolve()
    out_dir = Path(sys.argv[2]).resolve()
    tolerance = int(sys.argv[3])
    targets = [int(t.strip()) for t in sys.argv[4].split(",") if t.strip()]

    out_dir.mkdir(parents=True, exist_ok=True)

    started = time.perf_counter()
    output_file, total_rows = find_invoice_combinations_for_targets(
        str(xlsx_path), targets, tolerance, 5, str(out_dir)
    )
    elapsed = time.perf_counter() - started

    print(f"PYTHON rows={total_rows} time={elapsed:.2f}s")
    if output_file:
        print(f"OUTPUT:{output_file}")
    else:
        print("OUTPUT:none")


if __name__ == "__main__":
    main()
