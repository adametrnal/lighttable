#!/usr/bin/env python3
"""Install a per-user Lighttable launcher and icons for an existing binary.

Use --data-home /tmp/lighttable-desktop-check to stage the files for inspection.
The launcher points to the binary in place; rebuilding updates what it launches.
"""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
APP_ID = "lighttable-catalog"


def quote_executable(path):
    value = str(path)
    if any(char in value for char in "=%") or any(ord(char) < 32 for char in value):
        raise ValueError("Use an executable path without '=', '%' or control characters for desktop compatibility.")
    # Desktop Entry has two escape layers: string values, then Exec quoting.
    # This is not shell syntax and no shell is used to launch the application.
    value = "".join("\\" + char if char in '\\"`$' else char for char in value)
    return '"' + value.replace("\\", "\\\\") + '"'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    bundled = ROOT / "bin/lighttable"
    parser.add_argument("--binary", type=Path, default=bundled if bundled.is_file() else ROOT / "target/release/lighttable")
    parser.add_argument("--data-home", type=Path, help="Override the XDG user data directory (also useful for staging)")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f"Build an executable release binary first: {binary}")
    try:
        command = quote_executable(binary)
    except ValueError as error:
        parser.error(str(error))

    xdg = Path(os.environ.get("XDG_DATA_HOME", ""))
    default_data = xdg if xdg.is_absolute() else Path.home() / ".local/share"
    data = (args.data_home or default_data).expanduser().resolve()
    template = (ROOT / f"assets/linux/{APP_ID}.desktop").read_text(encoding="utf-8")
    desktop = template.replace("Exec=lighttable %f", f"Exec={command} %f")
    icons = ROOT / "assets/icons/hicolor"
    sources = sorted(icons.rglob(f"{APP_ID}*"))
    if len(sources) != 10 or any(not path.is_file() for path in sources):
        parser.error("Expected eight PNG icons and two SVG icons; run scripts/build-icons.py first.")

    # Validate before modifying the user's applications directory when available.
    validator = shutil.which("desktop-file-validate")
    if validator:
        with tempfile.TemporaryDirectory(prefix="lighttable-launcher-") as staging:
            staged = Path(staging) / f"{APP_ID}.desktop"
            staged.write_text(desktop, encoding="utf-8")
            subprocess.run([validator, str(staged)], check=True)
    for source in sources:
        destination = data / "icons/hicolor" / source.relative_to(icons)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        destination.chmod(0o644)
    launcher = data / "applications" / f"{APP_ID}.desktop"
    launcher.parent.mkdir(parents=True, exist_ok=True)
    launcher.write_text(desktop, encoding="utf-8")
    launcher.chmod(0o644)
    # Changing the top-level mtime tells icon-theme implementations to rescan.
    os.utime(data / "icons/hicolor", None)
    print(f"Installed launcher: {launcher}")
    print(f"Installed ten icon assets under: {data / 'icons/hicolor'}")
    print(f"Launcher executable: {binary}")


if __name__ == "__main__":
    main()
