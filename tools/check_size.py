from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
LIMIT = 20_000
EXTENSIONS = {".rs", ".wesl", ".wgsl"}

def source_counts():
    return {
        path.relative_to(ROOT).as_posix(): len(path.read_text(encoding="utf-8").splitlines())
        for crate in (ROOT / "crates").iterdir()
        for path in (crate / "src").rglob("*")
        if path.is_file() and path.suffix in EXTENSIONS
    }

if __name__ == "__main__":
    counts = source_counts()
    total = sum(counts.values())
    print(f"{total:,} / {LIMIT:,} source lines in {len(counts)} files")
    print("Includes blank lines, comments and inline tests; excludes assets and build outputs.")
    if "--files" in sys.argv:
        for path, lines in sorted(counts.items(), key=lambda item: (-item[1], item[0])):
            print(f"{lines:5}  {path}")
    if total > LIMIT:
        print(f"OVER BUDGET by {total - LIMIT:,} lines", file=sys.stderr)
        sys.exit(1)
