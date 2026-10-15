"""Round-corner the new TokenOwl icon with an Apple-style squircle mask.

macOS/iOS app icons use a continuous-curvature "squircle", not a plain
rounded rectangle; a circular-corner mask reads subtly wrong beside real
system icons. The superellipse below is the standard approximation.
"""

from PIL import Image, ImageDraw
import math

SOURCE = r"D:/AP/EF0AC710-6625-4963-A213-8BBA66D441B1.png"
OUT_ROUNDED = r"D:/AI/TokenOwl/tokenowl/.icon-rounded.png"

# Apple's icon grid: the rounded shape occupies ~82.4% of the canvas with
# ~9% padding on each side at the standard 1024 canvas.
CANVAS = 1024
INSET = 92  # ~9% padding
RADIUS_RATIO = 0.2237  # Apple squircle corner radius as a fraction of size
SUPERELLIPSE_N = 5.0  # exponent controlling corner continuity


def squircle_mask(size: int, inset: int) -> Image.Image:
    """White squircle on black, antialiased by drawing at 4x then downscaling."""
    scale = 4
    big = size * scale
    mask = Image.new("L", (big, big), 0)
    draw = ImageDraw.Draw(mask)
    side = big - inset * scale * 2
    left = inset * scale
    top = inset * scale
    # Superellipse: |x/a|^n + |y/b|^n = 1 sampled around the perimeter
    points = []
    steps = 720
    a = side / 2
    for i in range(steps):
        theta = 2 * math.pi * i / steps
        cos_t, sin_t = math.cos(theta), math.sin(theta)
        x = math.copysign(abs(cos_t) ** (2 / SUPERELLIPSE_N), cos_t) * a
        y = math.copysign(abs(sin_t) ** (2 / SUPERELLIPSE_N), sin_t) * a
        points.append((left + a + x, top + a + y))
    draw.polygon(points, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def main() -> None:
    source = Image.open(SOURCE).convert("RGBA")
    # Fit the artwork into the full canvas, then apply the padded squircle so
    # the shape keeps Apple's margin proportions.
    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    art = source.resize((CANVAS, CANVAS), Image.LANCZOS)
    canvas.paste(art, (0, 0))
    mask = squircle_mask(CANVAS, INSET)
    canvas.putalpha(mask)
    canvas.save(OUT_ROUNDED, "PNG")
    print(f"wrote {OUT_ROUNDED} at {canvas.size}")


if __name__ == "__main__":
    main()
