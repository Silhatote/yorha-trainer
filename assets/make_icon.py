"""Draws the app icon (a lit diamond on a dark tile) and writes yorha-trainer.ico and
icon-256.png next to this script. Each size is drawn on its own, 4x supersampled, so the
small ones stay crisp.

    python assets/make_icon.py
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

HERE = Path(__file__).resolve().parent
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

DARK = (58, 55, 48, 255)
DARKER = (40, 38, 33, 255)
PAPER = (221, 215, 190, 255)
GLOW = (168, 216, 255)


def diamond(cx, cy, r):
    return [(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)]


def draw(size: int) -> Image.Image:
    ss = 4
    n = size * ss
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    small = size <= 24

    # Tile: dark, with a thin paper rule inside (dropped at tiny sizes).
    radius = n * (0.16 if size >= 32 else 0.12)
    d.rounded_rectangle([0, 0, n - 1, n - 1], radius=radius, fill=DARK)
    # A soft shade toward the bottom.
    shade = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    for y in range(n):
        a = int(70 * max(0.0, (y / n - 0.35) / 0.65))
        ImageDraw.Draw(shade).line([(0, y), (n, y)], fill=DARKER[:3] + (a,))
    mask = Image.new("L", (n, n), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, n - 1, n - 1], radius=radius, fill=255)
    img.paste(shade, (0, 0), Image.composite(shade, Image.new("RGBA", (n, n)), mask).getchannel("A"))
    d = ImageDraw.Draw(img)
    if not small:
        inset = n * 0.07
        d.rounded_rectangle(
            [inset, inset, n - 1 - inset, n - 1 - inset],
            radius=radius * 0.6,
            outline=PAPER[:3] + (90,),
            width=max(ss, int(n * 0.012)),
        )

    c = n / 2
    # Cold glow behind the core.
    glow = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gr = n * 0.26
    gd.ellipse([c - gr, c - gr, c + gr, c + gr], fill=GLOW + (150,))
    glow = glow.filter(ImageFilter.GaussianBlur(n * 0.09))
    img = Image.alpha_composite(img, glow)
    d = ImageDraw.Draw(img)

    # Outer diamond (outline), inner diamond (filled), lit core.
    outer = n * (0.36 if small else 0.33)
    width = int(n * (0.085 if small else 0.05))
    d.polygon(diamond(c, c, outer), outline=PAPER, width=width)
    inner = n * (0.15 if small else 0.13)
    d.polygon(diamond(c, c, inner), fill=PAPER)
    if not small:
        core = n * 0.055
        d.polygon(diamond(c, c, core), fill=GLOW + (255,))
        # Four ticks at the tips, like the interface's corner marks.
        t = n * 0.06
        for (x, y) in diamond(c, c, outer + n * 0.08):
            d.rectangle([x - t / 3, y - t / 3, x + t / 3, y + t / 3], fill=PAPER[:3] + (200,))

    return img.resize((size, size), Image.LANCZOS)


def main():
    images = [draw(s) for s in SIZES]
    big = images[-1]
    big.save(HERE / "icon-256.png")
    big.save(HERE / "yorha-trainer.ico", sizes=[(s, s) for s in SIZES], append_images=images[:-1])
    print("wrote", HERE / "yorha-trainer.ico", "and", HERE / "icon-256.png")


if __name__ == "__main__":
    main()
