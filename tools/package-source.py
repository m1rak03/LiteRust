"""Build a source archive locally. Does not access network services."""
import argparse
import hashlib
from pathlib import Path
import tomllib
import zipfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output-dir', type=Path, default=root.parent / 'output')
args = parser.parse_args()
output = args.output_dir.resolve()
if output == root or root in output.parents:
    parser.error('Output directory must be outside the source tree')
output.mkdir(parents=True, exist_ok=True)
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
archive = output / f'literust-browser-{version}-source.zip'
skip = {'target', '.git', 'node_modules', 'audit-logs', '__pycache__', 'release-source', 'legacy'}
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as z:
    for path in sorted(root.rglob('*')):
        relative = path.relative_to(root)
        if (not path.is_file() or path.is_symlink() or skip.intersection(relative.parts)
                or path.suffix in {'.pyc', '.pending', '.log'}):
            continue
        z.write(path, 'literust-browser/' + relative.as_posix())
with zipfile.ZipFile(archive) as z:
    broken = z.testzip()
    if broken:
        raise RuntimeError(f'Archive verification failed: {broken}')
with archive.open('rb') as stream:
    digest = hashlib.file_digest(stream, 'sha256').hexdigest()
checksum = archive.with_suffix('.zip.sha256')
checksum.write_text(f'{digest}  {archive.name}\n')
print(archive)
print(f'{archive.stat().st_size} bytes; SHA-256 {digest}')
