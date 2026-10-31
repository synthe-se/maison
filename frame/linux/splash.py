"""The frame's boot picture, drawn by the initramfs on /dev/fb0 as soon as the panel is up (U-Boot
draws nothing from our boot image): Maison's mark on the app's ground and nothing else; a line
under it only when the boot fails (« Système introuvable… »), a strip of its own copied over the
picture. Raw XRGB8888 as the framebuffer holds it (800×1280, 3200 bytes a row), turned 180° (the
panel is mounted upside down: what the person sees is the framebuffer's last row first). The
colours are the app's tokens (web/src/styles/tokens.css), the mark the web app's
(web/static/brand.svg), the words i18n/messages/fr.json's (the house's language). Rendered with
rsvg-convert, read back with build.py's PNG reader.
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "frame/system"))
import build  # noqa: E402

W, H = 800, 1280
STRIDE = W * 4
MARK = 240
# the failure's line, under the mark (in what the person sees)
STRIP_Y, STRIP_H = 900, 64
# the framebuffer's row where the strip starts once turned
STRIP_ROW = H - (STRIP_Y + STRIP_H)
# the failure's words, by key (the initramfs shows them by the key's last word)
STEPS = ("frame_boot_missing",)


def token(name: str) -> str:
    """A raw colour of tokens.css (`--raw-cloud: #f2f0ea;`)."""
    css = (REPO / "web/src/styles/tokens.css").read_text()
    found = re.search(rf"--raw-{name}:\s*(#[0-9a-fA-F]{{6}})", css)
    if not found:
        raise SystemExit(f"tokens.css: no --raw-{name}")
    return found.group(1)


def render(svg: str, w: int, h: int) -> list[bytes]:
    """`svg` as RGB rows, `w`×`h`."""
    with tempfile.TemporaryDirectory() as d:
        src, out = Path(d, "in.svg"), Path(d, "out.png")
        src.write_text(svg)
        subprocess.run(["rsvg-convert", "-w", str(w), "-h", str(h), "-o", str(out), str(src)], check=True)
        rw, rh, rows = build.png_rows(out.read_bytes())
    if (rw, rh) != (w, h):
        raise SystemExit(f"splash: {rw}×{rh} rendered, {w}×{h} wanted")
    return rows


def raw(rows: list[bytes]) -> bytes:
    """RGB rows as the framebuffer's XRGB8888 (bytes B, G, R, X), turned 180°."""
    out = bytearray()
    for row in reversed(rows):
        px = [row[i : i + 3] for i in range(0, len(row), 3)]
        for r, g, b in reversed(px):
            out += bytes((b, g, r, 0))
    return bytes(out)


def frames() -> dict[str, bytes]:
    """`splash` (the whole picture) and the failure's strip, by its key's last word."""
    ground, ink = token("cloud"), token("ink-soft")
    mark = (REPO / "web/static/brand.svg").read_text()
    mark = mark[mark.index("<svg") :]
    inner = mark[mark.index(">") + 1 : mark.rindex("</svg>")]
    x, y = (W - MARK) // 2, (H - MARK) // 2 - 80
    splash = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">'
        f'<rect width="{W}" height="{H}" fill="{ground}"/>'
        f'<g transform="translate({x} {y}) scale({MARK / 48})">{inner}</g></svg>'
    )
    words = __import__("json").loads((REPO / "i18n/messages/fr.json").read_text())
    out = {"splash": raw(render(splash, W, H))}
    for key in STEPS:
        text = words[key].replace("&", "&amp;").replace("<", "&lt;")
        strip = (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{STRIP_H}">'
            f'<rect width="{W}" height="{STRIP_H}" fill="{ground}"/>'
            f'<text x="{W // 2}" y="{STRIP_H // 2 + 9}" text-anchor="middle" fill="{ink}" '
            f'font-family="Helvetica Neue, Helvetica, Arial, sans-serif" font-size="26">{text}</text></svg>'
        )
        out[key.rsplit("_", 1)[1]] = raw(render(strip, W, STRIP_H))
    return out


if __name__ == "__main__":
    for name, data in frames().items():
        print(name, len(data))
