import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('release', ROOT/'scripts/package-release.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class SourceExportTests(unittest.TestCase):
    def test_private_data_and_build_outputs_are_not_exported(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)/'checkout'; root.mkdir()
            (root/'release-files.txt').write_text('README.md\nsrc/*.rs\n')
            (root/'README.md').write_text('Public project\n')
            (root/'src').mkdir(); (root/'src/main.rs').write_text('fn main() {}\n')
            for name in ['.test-data/private.lrcat', '.tools/private-key', 'target/app',
                         'state/selections.sqlite', '.env', 'personal.jpg']:
                p=root/name; p.parent.mkdir(parents=True,exist_ok=True); p.write_text('private')
            output=Path(temp)/'export'
            manifest=release.export_source(output, root)
            self.assertEqual(set(manifest), {'README.md','src/main.rs'})
            self.assertEqual(set(p.relative_to(output).as_posix() for p in output.rglob('*') if p.is_file()),
                             {'README.md','src/main.rs','SOURCE-MANIFEST.json'})
            self.assertEqual(json.loads((output/'SOURCE-MANIFEST.json').read_text()),manifest)

    def test_missing_patterns_and_symlinks_fail_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'checkout'; root.mkdir()
            (root/'release-files.txt').write_text('src/*.rs\n')
            with self.assertRaises(ValueError): release.public_files(root)
            (root/'src').mkdir()
            secret=Path(temp)/'secret';secret.write_text('private')
            (root/'src/main.rs').symlink_to(secret)
            with self.assertRaises(ValueError): release.public_files(root)

    def test_manifest_cannot_include_catalog_or_parent_paths(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            (root/'private.lrcat').write_text('private')
            for pattern in ['private.lrcat','../*']:
                (root/'release-files.txt').write_text(pattern+'\n')
                with self.assertRaises(ValueError): release.public_files(root)


if __name__ == '__main__':
    unittest.main()
