#!/usr/bin/env python3
"""Collect locked Linux crate and Rust library notices. Offline; fails on omissions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
EGUI = {'ecolor', 'eframe', 'egui', 'egui-wgpu', 'egui-winit', 'egui_glow', 'emath', 'epaint'}
FALLBACK = {'gl_generator': ('0.14.0', 'gl-generator'), 'khronos_api': ('3.1.0', 'khronos-api'),
            'hexf-parse': ('0.2.1', 'hexf-parse'), 'profiling': ('1.0.18', 'profiling')}


def cargo(*args):
    return subprocess.check_output(['bash', str(ROOT/'scripts/cargo-local'), *args], cwd=ROOT, text=True)


def generate(output):
    output.mkdir(parents=True, exist_ok=False)
    meta = json.loads(cargo('metadata', '--locked', '--offline', '--format-version', '1',
                            '--filter-platform', 'x86_64-unknown-linux-gnu'))
    selected = {n['id'] for n in meta['resolve']['nodes']}
    packages = sorted((p for p in meta['packages'] if p['id'] in selected and p['source']),
                      key=lambda p: (p['name'], p['version']))
    sources = json.loads((ROOT/'licenses/upstream/sources.json').read_text())
    for path, info in sources.items():
        if hashlib.sha256((ROOT/'licenses/upstream'/path).read_bytes()).hexdigest() != info['sha256']:
            raise ValueError(f'Changed upstream notice: {path}')
    index = ['# Third-party notices', '',
             'Generated from Cargo.lock for x86_64-unknown-linux-gnu. Includes resolved build/optional crates.',
             'Files retain their upstream copyright and license terms. See THIRD_PARTY.md for scope.', '',
             '| Package | Declared license | Notices |', '| --- | --- | --- |']
    inventory = []
    for p in packages:
        root = Path(p['manifest_path']).parent
        files = [f for f in root.rglob('*') if f.is_file() and
                 (any(s in f.name.lower() for s in ('license', 'copying', 'notice', 'copyright', 'unlicense'))
                  or ('fonts' in f.parts and f.suffix == '.txt'))]
        if p['license_file']:
            files.append(root/p['license_file'])
        if not files:
            expected, folder = ('0.33.3', 'egui') if p['name'] in EGUI else FALLBACK.get(p['name'], (None, None))
            if p['version'] != expected:
                raise ValueError(f"Missing reviewed notices: {p['name']} {p['version']}")
            root = ROOT/'licenses/upstream'/folder
            files = [f for f in root.rglob('*') if f.is_file()]
        if not p['license'] or not files:
            raise ValueError(f"Missing license metadata/files: {p['name']}")
        name = f"{p['name']}-{p['version']}"
        copied = []
        for f in sorted(set(files)):
            rel = f.relative_to(root)
            if f.is_symlink() or not f.resolve().is_relative_to(root.resolve()):
                raise ValueError(f'Unexpected notice symlink: {f}')
            dest = output/'licenses'/name/rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(f, dest)
            copied.append(dest.relative_to(output).as_posix())
        if p['name'] == 'epaint_default_fonts':
            if p['version'] != '0.33.3':
                raise ValueError('Review the egui code license for the new font-crate version')
            dest = output/'licenses'/name/'CODE-LICENSE-MIT'
            shutil.copyfile(ROOT/'licenses/upstream/egui/LICENSE-MIT', dest)
            copied.append(dest.relative_to(output).as_posix())
        if p['name'] == 'libsqlite3-sys':
            header = (Path(p['manifest_path']).parent/'sqlite3/sqlite3.h').read_text()
            notice = header[:header.index('*/')+2]
            if 'author disclaims copyright' not in notice:
                raise ValueError('Review the bundled SQLite source notice')
            dest = output/'licenses'/name/'SQLITE-NOTICE.txt'
            dest.write_text(notice+'\n')
            copied.append(dest.relative_to(output).as_posix())
        index.append(f"| {name} | {p['license']} | [files](licenses/{name}/) |")
        inventory.append({'name': p['name'], 'version': p['version'], 'license': p['license'],
                          'repository': p['repository'], 'authors': p['authors'], 'files': copied})
    # std and compiler runtime notices for the actual build toolchain.
    environment = os.environ.copy()
    rustc = 'rustc'
    if (ROOT/'.tools/cargo/bin/rustc').is_file():
        rustc = str(ROOT/'.tools/cargo/bin/rustc')
        environment.update(RUSTUP_HOME=str(ROOT/'.tools/rustup'), CARGO_HOME=str(ROOT/'.tools/cargo'))
    toolchain = subprocess.check_output([rustc, '--print', 'sysroot'], env=environment,
                                        cwd=ROOT, text=True).strip()
    rust = Path(toolchain)/'share/doc/rust'
    library = rust/'COPYRIGHT-library.html'
    if not library.is_file() or not (rust/'licenses').is_dir():
        raise ValueError('Install a Rust toolchain with standard-library copyright notices')
    dest = output/'licenses/rust'
    dest.mkdir(parents=True)
    shutil.copyfile(library, dest/library.name)
    shutil.copytree(rust/'licenses', dest/'licenses')
    index.extend(['', 'Rust standard-library and runtime notices: [copyright inventory](licenses/rust/COPYRIGHT-library.html).',
                  'Upstream replacement notice provenance: [sources](licenses/upstream-sources.json).', ''])
    (output/'licenses/upstream-sources.json').write_text(json.dumps(sources, indent=2)+'\n')
    (output/'THIRD_PARTY_NOTICES.md').write_text('\n'.join(index))
    (output/'licenses/inventory.json').write_text(json.dumps(inventory, indent=2)+'\n')
    print(f'Collected notices for {len(packages)} Linux crates, embedded fonts and Rust libraries.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    generate(parser.parse_args().output.resolve())
