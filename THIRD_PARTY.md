# Third-party software

Lighttable's own code, artwork and generated test imagery are covered by
[LICENSE](LICENSE). Dependency and font licenses retain their own terms.

Every binary bundle contains `THIRD_PARTY_NOTICES.md` and a `licenses/` directory
generated from the locked Linux dependency graph, the exact crate source packages
and the Rust standard-library notices. The inventory includes build dependencies
and optional resolved crates as a conservative superset of linked code. It records
package names, versions, declared license expressions and upstream repositories.

The embedded egui fonts include Hack, Noto Emoji, Ubuntu Light and emoji-icon-font.
Their actual notice files are included, including the SIL Open Font License and
Ubuntu Font Licence. Little CMS and the bundled SQLite source have separate
notices. System libraries such as glibc, Little CMS, Mesa, X11 and Wayland are
provided by the Linux distribution and are not copied into the tarball.

Some published crates omit their root license files. Verified copies from their
exact upstream commits, or canonical SPDX license text, are retained under
`licenses/upstream/`, with source URLs and hashes in `licenses/upstream/sources.json`.
The notice generator fails if a resolved
crate has neither packaged notices nor a known replacement.

Regenerate notices for the release platform with:

```bash
python3 scripts/generate-notices.py /tmp/lighttable-notices-new
```

This uses `cargo metadata --locked --offline`; run `cargo fetch --locked` first
when the crate cache is empty. `Cargo.lock` is the dependency version authority.

Lighttable is an independent project and is not affiliated with or endorsed by
Adobe. Lightroom and Lightroom Classic are Adobe product names.
