# Preparing a release

The target repository is [adametrnal/lighttable](https://github.com/adametrnal/lighttable).
Version 0.1.0 is an **alpha prerelease**. Preparation builds and verifies artifacts;
publication is a separate maintainer action.

## Clean Linux build

From the project root with Python 3.11+ and Docker with Buildx available:

```bash
python3 scripts/package-release.py source /tmp/lighttable-source-new
docker build --target artifacts --output type=local,dest=dist/ubuntu \
  -f /tmp/lighttable-source-new/packaging/Dockerfile /tmp/lighttable-source-new
```

Use fresh output directories. The explicit `release-files.txt` inventory is the
source export authority; unmatched patterns, symlinks and catalog/state paths
are rejected. The Docker context contains only that exported source, with no
local toolchain or build cache. The root `.dockerignore` also excludes private data.
BuildKit can reuse its own dependency/build caches between runs; the initial
validation started with empty container caches. These are separate from `.tools/`.

The Ubuntu 24.04 builder installs the toolchain pinned in `rust-toolchain.toml`,
uses `Cargo.lock`, runs `scripts/check.sh`, builds the optimized binary, generates
dependency/font notices, creates archives, verifies checksums and tests the desktop
workflow with generated imagery. A second Ubuntu image runs the packaged app with
runtime libraries and Xvfb, without compiler/development packages.

Outputs are the Linux x86_64 tarball, source tarball, Lightroom companion ZIP,
`SHA256SUMS`, and validation receipts. Archives normalize timestamps/ownership;
this is not a claim that compiler output or moving distribution packages are
bit-for-bit reproducible. `BUILD-INFO.json` records the actual build environment.

To package a native local build instead, install the build/test dependencies from
README and run `bash scripts/validate-release.sh /tmp/lighttable-release-new`.
Only the Ubuntu recipe has the documented Ubuntu runtime baseline; a host-built
tarball may require newer libraries and must not be substituted silently.

## Release review

1. Update Cargo/package and companion versions together, and refresh `Cargo.lock`.
2. Update release notes and the measured results in `RELEASE_VALIDATION.md`.
3. Verify all source and binary archive members against the explicit source inventory.
   Never add personal footage or a real catalog to make a screenshot look better.
4. Regenerate notices after dependency changes. Missing upstream notices stop packaging.
5. Confirm the clean build, runtime smoke checks and SHA-256 checks pass. Inspect the public synthetic screenshot.
6. Review the exact source commit and assets, then publish only the intended prerelease.

The public screenshot can be regenerated with `scripts/make-demo-catalog.py` and
the native `--smoke-test` harness. Its source imagery is generated test data under
the project license. The accepted private scrolling video remains excluded.

## GitHub workflows

CI runs the clean container recipe for `main`, pull requests and manual runs,
then uploads the verified artifacts. Third-party actions are pinned to full
commit SHAs, and checkout does not persist credentials, following
[GitHub's workflow security guidance](https://docs.github.com/en/actions/reference/security/secure-use).

The release workflow runs on a `v*` tag or a manual run **at that tag**. It requires
the tag to equal `v` plus the Cargo version, rebuilds and verifies the artifacts,
then creates an **unpublished draft marked as a prerelease** using the release notes.
It does not publish the draft. Repository Actions must be enabled; the release job
needs the repository's `contents: write` permission. A pre-existing release causes
the creation step to fail instead of replacing it.

After reviewing the prepared source, a maintainer can push it to the empty target
repository, tag the reviewed commit as `v0.1.0`, and push that tag. Wait for CI and
the draft-release workflow before reviewing and publishing the draft in GitHub.
An authenticated GitHub CLI or browser session is needed to manage publication.
