#!/usr/bin/env python3
"""Derive Lighttable's vector variants and PNGs from the original slide mark.

Requires Python 3 and rsvg-convert (librsvg); no Python packages or fonts are
needed for the icons. DejaVu Sans is used only for labels on the preview sheet.
"""

import copy
from pathlib import Path
import shutil
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "assets/branding"
ICONS = ROOT / "assets/icons/hicolor"
NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", NS)


def element(tag, **attrs):
    return ET.Element(f"{{{NS}}}{tag}", attrs)


def write_svg(path, svg):
    path.parent.mkdir(parents=True, exist_ok=True)
    ET.indent(svg, space="  ")
    ET.ElementTree(svg).write(path, encoding="utf-8", xml_declaration=True)


def render(source, target, size=None):
    target.parent.mkdir(parents=True, exist_ok=True)
    args = ["rsvg-convert", str(source), "--output", str(target)]
    if size:
        args += ["--width", str(size), "--height", str(size)]
    subprocess.run(args, check=True)


def recolor(svg, colors):
    variant = copy.deepcopy(svg)
    for node in variant.iter():
        if node.get("fill") in colors:
            node.set("fill", colors[node.get("fill")])
    return variant


def embed(sheet, svg, x, y, size, ident):
    # Each embedded mark needs its own mask ID on the shared proof sheet.
    mark = copy.deepcopy(svg)
    mark.attrib.update(x=str(x), y=str(y), width=str(size), height=str(size))
    for node in mark.iter():
        if node.get("id"):
            node.set("id", f"{ident}-{node.get('id')}")
        for attr, value in list(node.attrib.items()):
            if value.startswith("url(#"):
                node.set(attr, value.replace("url(#", f"url(#{ident}-"))
    sheet.append(mark)


def label(sheet, x, y, text, size=14, color="#9DA6B2", **attrs):
    node = element("text", x=str(x), y=str(y), fill=color,
                   **{"font-family": "DejaVu Sans, sans-serif", "font-size": str(size), **attrs})
    node.text = text
    sheet.append(node)


def main():
    if not shutil.which("rsvg-convert"):
        raise SystemExit("Install librsvg's rsvg-convert to regenerate the icons.")

    master = ET.parse(BRAND / "lighttable-mark.svg").getroot()
    light = recolor(master, {"#FFF0CC": "#414B59", "#E2B76B": "#94631D"})
    symbolic = recolor(master, {"#FFF0CC": "#222222", "#E2B76B": "#222222"})
    write_svg(BRAND / "lighttable-mark-light.svg", light)
    write_svg(ICONS / "symbolic/apps/lighttable-catalog-symbolic.svg", symbolic)

    icon = copy.deepcopy(master)
    tile = element("rect", x="1", y="1", width="62", height="62", rx="14", fill="#242B34")
    icon.insert(3, tile)
    icon_path = ICONS / "scalable/apps/lighttable-catalog.svg"
    write_svg(icon_path, icon)
    for size in (16, 24, 32, 48, 64, 128, 256, 512):
        render(icon_path, ICONS / f"{size}x{size}/apps/lighttable-catalog.png", size)
    render(BRAND / "lighttable-mark.svg", BRAND / "lighttable-mark.png", 128)

    # A compact review sheet: the mark, both backgrounds and native icon sizes.
    sheet = element("svg", width="1000", height="580", viewBox="0 0 1000 580")
    sheet.append(element("rect", width="1000", height="580", rx="24", fill="#11151A"))
    embed(sheet, icon, 48, 48, 200, "hero")
    label(sheet, 296, 137, "LIGHTTABLE", 48, "#F3EFE5", **{"letter-spacing": "3"})
    label(sheet, 299, 177, "A little light for your photographs.", 18)
    sheet.append(element("path", d="M48 284H952", stroke="#303740"))
    label(sheet, 48, 324, "THE MARK", 12, **{"letter-spacing": "2"})
    sheet.append(element("rect", x="48", y="348", width="150", height="150", rx="18", fill="#1B2027"))
    sheet.append(element("rect", x="216", y="348", width="150", height="150", rx="18", fill="#F5F1E8"))
    embed(sheet, master, 75, 375, 96, "dark")
    embed(sheet, light, 243, 375, 96, "light")
    label(sheet, 123, 524, "Dark", 12, **{"text-anchor": "middle"})
    label(sheet, 291, 524, "Light", 12, **{"text-anchor": "middle"})
    label(sheet, 422, 324, "APP ICON · ACTUAL SIZES", 12, **{"letter-spacing": "2"})
    for x, size in ((432, 16), (489, 24), (554, 32), (627, 48), (716, 64), (821, 128)):
        embed(sheet, icon, x, 423 - size / 2, size, f"size{size}")
        label(sheet, x + size / 2, 524, str(size), 12, **{"text-anchor": "middle"})
    write_svg(BRAND / "preview.svg", sheet)
    render(BRAND / "preview.svg", BRAND / "preview.png")
    print("Generated SVG variants, eight PNG icon sizes, header mark and preview sheet.")


if __name__ == "__main__":
    main()
