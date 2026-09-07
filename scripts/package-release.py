#!/usr/bin/env python3
"""Export an allowlisted source tree or package a native Linux build. Never publish."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def public_files(root=ROOT):
    result = set()
    for line in (root/'release-files.txt').read_text().splitlines():
        pattern = line.strip()
        if not pattern or pattern.startswith('#'):
            continue
        if pattern.startswith('/') or '..' in Path(pattern).parts:
            raise ValueError(f'Unsafe source pattern: {pattern}')
        matches = sorted(root.glob(pattern))
        if not matches:
            raise ValueError(f'Public source pattern matched nothing: {pattern}')
        for path in matches:
            if not path.is_file() or path.resolve() != path.absolute():
                raise ValueError(f'Public source must be an ordinary file: {path}')
            rel = path.relative_to(root)
            if any(p in {'.test-data', '.tools', '.git', 'target', 'dist'} for p in rel.parts):
                raise ValueError(f'Private/build directory in source manifest: {rel}')
            if any(s in path.name.lower() for s in ('.lrcat', '.lrdata', '.sqlite', '.dng', '.cr2', '.cr3')):
                raise ValueError(f'Catalog, state or original in source manifest: {rel}')
            result.add(rel)
    return sorted(result)


def export_source(output, root=ROOT):
    files = public_files(root)
    output.mkdir(parents=True, exist_ok=False)
    manifest = {}
    for rel in files:
        dest = output/rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(root/rel, dest)
        dest.chmod(0o755 if (root/rel).stat().st_mode & 0o111 else 0o644)
        manifest[rel.as_posix()] = digest(dest)
    (output/'SOURCE-MANIFEST.json').write_text(json.dumps(manifest, indent=2)+'\n')
    return manifest


def archive(folder, output, prefix):
    # Stable metadata makes archives reproducible for identical staged contents.
    with output.open('xb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode='w') as tar:
            for path in sorted(folder.rglob('*')):
                if not path.is_file() or path.is_symlink():
                    continue
                info = tar.gettarinfo(str(path), arcname=f'{prefix}/{path.relative_to(folder).as_posix()}')
                info.uid = info.gid = 0
                info.uname = info.gname = ''
                info.mtime = 0
                info.mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
                with path.open('rb') as f:
                    tar.addfile(info, f)


def build(output, binary):
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise ValueError('This release recipe currently packages native Linux x86_64 only')
    version = tomllib.loads((ROOT/'Cargo.toml').read_text())['package']['version']
    binary = binary.resolve(strict=True)
    if subprocess.check_output([str(binary), '--version'], text=True).strip() != f'Lighttable {version}':
        raise ValueError('Binary version does not match Cargo.toml')
    deps = subprocess.check_output(['ldd', str(binary)], text=True)
    deps = re.sub(r' \(0x[0-9a-f]+\)', '', deps)
    if 'not found' in deps:
        raise ValueError('Binary has unresolved shared libraries')
    output.mkdir(parents=True, exist_ok=False)
    manifest = export_source(output/'source')
    bundle = output/'bundle'
    subprocess.run(['python3', str(ROOT/'scripts/generate-notices.py'), str(bundle)], check=True)
    for name in ['LICENSE', 'THIRD_PARTY.md']:
        shutil.copyfile(ROOT/name, bundle/name)
    shutil.copyfile(ROOT/'packaging/README-LINUX.md', bundle/'README.md')
    for name in ['assets/icons', 'assets/linux', 'lightroom']:
        shutil.copytree(output/'source'/name, bundle/name)
    (bundle/'scripts').mkdir()
    shutil.copyfile(ROOT/'scripts/install-desktop.py', bundle/'scripts/install-desktop.py')
    (bundle/'bin').mkdir()
    shutil.copyfile(binary, bundle/'bin/lighttable')
    (bundle/'bin/lighttable').chmod(0o755)
    (bundle/'SOURCE-MANIFEST.json').write_text(json.dumps(manifest, indent=2)+'\n')
    rust = subprocess.check_output(['bash', str(ROOT/'scripts/cargo-local'), '--version'], text=True).strip()
    info = {'version': version, 'target': 'x86_64-unknown-linux-gnu', 'cargo': rust,
            'build_os': Path('/etc/os-release').read_text(), 'glibc': platform.libc_ver(),
            'binary_sha256': digest(binary), 'source_manifest_sha256': digest(bundle/'SOURCE-MANIFEST.json'),
            'dynamic_libraries': deps, 'source_repository': 'https://github.com/adametrnal/lighttable'}
    (bundle/'BUILD-INFO.json').write_text(json.dumps(info, indent=2)+'\n')
    archive(bundle, output/f'lighttable-{version}-linux-x86_64.tar.gz', f'lighttable-{version}-linux-x86_64')
    archive(output/'source', output/f'lighttable-{version}-source.tar.gz', f'lighttable-{version}')
    with zipfile.ZipFile(output/f'lighttable-{version}-lightroom-plugin.zip', 'x', zipfile.ZIP_DEFLATED) as z:
        for p in sorted((bundle/'lightroom').rglob('*.lua')):
            z.writestr(zipfile.ZipInfo(p.relative_to(bundle/'lightroom').as_posix(), (2026,1,1,0,0,0)), p.read_bytes())
        z.writestr(zipfile.ZipInfo('Lighttable.lrplugin/LICENSE', (2026,1,1,0,0,0)), (ROOT/'LICENSE').read_bytes())
    artifacts = sorted([*output.glob('*.tar.gz'), *output.glob('*.zip')])
    (output/'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in artifacts))
    print('\n'.join(str(p) for p in artifacts))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['source', 'build'])
    parser.add_argument('output', type=Path)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/lighttable')
    args = parser.parse_args()
    if args.mode == 'source':
        files = export_source(args.output.resolve())
        print(f'Exported {len(files)} public files to {args.output.resolve()}')
    else:
        build(args.output.resolve(), args.binary)
