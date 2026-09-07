# Lighttable 0.1.0 — Linux alpha

This is the x86_64 Linux tarball built on Ubuntu 24.04. It requires glibc 2.39
or newer, Little CMS 2, a Wayland or X11 desktop and working OpenGL drivers.
It is not a static binary or an AppImage. Other distributions are not yet certified.

On Ubuntu 24.04, install the runtime libraries if they are missing:

```bash
sudo apt-get install liblcms2-2 libegl1 libgl1 libwayland-client0 libxkbcommon0 \
  libxkbcommon-x11-0 libx11-6 libxi6 libxrandr2 libxcursor1 libxinerama1
```

Extract the archive somewhere permanent and run:

```bash
./bin/lighttable
./bin/lighttable "/path/to/My Catalog.lrcat"
```

With no arguments, use **Open catalog…** in the app. Close Lightroom before
copying a catalog and keep its matching `Previews.lrdata` directory beside it.
Try a disposable copy first. The app reads a private snapshot and stores local
ratings/flags separately. It does not apply SQL edits to the original catalog.

For a catalog with a single photo root, `--photo-root /mnt/photos` maps that root
to Linux. Use `--data-dir /path/to/state` for an isolated selection profile.
Default selections live in `$XDG_DATA_HOME/lighttable/selections.sqlite`, usually
`~/.local/share/lighttable/selections.sqlite`. Keep this database to retain edits.

Keyboard: **G/E** grid/loupe, **arrows** navigate, **0–5** stars,
**P/X/U** pick/reject/unflag, **Ctrl+Z** undo. Rating and flag edits affect all
selected photos. **Delete** opens the file review; only its final confirmation
moves files to desktop Trash. Catalog entries remain in Lightroom.

Optional desktop launcher (requires Python 3):

```bash
python3 scripts/install-desktop.py
```

The launcher points to this extracted directory. Keep it in place. To uninstall,
remove the launcher/icons it reports and the extracted directory. Preserve the
selection database unless you intentionally want to discard local selections.

The experimental Lightroom importer is in `lightroom/Lighttable.lrplugin`.
Its parser/conflict tests pass, but Lightroom-side validation is incomplete.
Direct catalog sync remains experimental research and is not an app feature.
Missing previews, unsupported raw formats and unsupported Smart Collection rules
are current alpha limitations. Full sustained browsing performance is unmeasured.

Source, usage details and issue reporting:
[github.com/adametrnal/lighttable](https://github.com/adametrnal/lighttable).
See `LICENSE`, `THIRD_PARTY.md`, `THIRD_PARTY_NOTICES.md`, `licenses/` and
`BUILD-INFO.json` for licensing and build provenance.
