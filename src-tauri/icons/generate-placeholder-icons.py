#!/usr/bin/env python3

from PIL import Image, ImageDraw, ImageFont
import os

def create_placeholder_icon(size, filename):
    img = Image.new('RGBA', (size, size), color=(66, 135, 245, 255))

    draw = ImageDraw.Draw(img)

    circle_margin = size // 8
    draw.ellipse([circle_margin, circle_margin, size - circle_margin, size - circle_margin],
                 fill=(255, 255, 255, 255))

    try:
        font_size = size // 3
        font = ImageFont.truetype("arial.ttf", font_size)
    except:
        font = ImageFont.load_default()

    text = "R"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_width = bbox[2] - bbox[0]
    text_height = bbox[3] - bbox[1]

    text_x = (size - text_width) // 2
    text_y = (size - text_height) // 2 - bbox[1]

    draw.text((text_x, text_y), text, fill=(66, 135, 245, 255), font=font)

    img.save(filename)
    print(f"Created: {filename}")

def create_ico(sizes, filename):
    images = []
    for size in sizes:
        img = Image.new('RGBA', (size, size), color=(66, 135, 245, 255))
        draw = ImageDraw.Draw(img)

        circle_margin = size // 8
        draw.ellipse([circle_margin, circle_margin, size - circle_margin, size - circle_margin],
                     fill=(255, 255, 255, 255))

        try:
            font_size = size // 3
            font = ImageFont.truetype("arial.ttf", font_size)
        except:
            font = ImageFont.load_default()

        text = "R"
        bbox = draw.textbbox((0, 0), text, font=font)
        text_width = bbox[2] - bbox[0]
        text_height = bbox[3] - bbox[1]

        text_x = (size - text_width) // 2
        text_y = (size - text_height) // 2 - bbox[1]

        draw.text((text_x, text_y), text, fill=(66, 135, 245, 255), font=font)
        images.append(img)

    images[0].save(filename, format='ICO', sizes=[(s, s) for s in sizes])
    print(f"Created: {filename}")

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))

    print("Generating placeholder icons for Lattice...")
    print("WARNING: These are PLACEHOLDER icons for development only!")
    print("Replace with final branding before production release.\n")

    create_placeholder_icon(32, os.path.join(script_dir, "32x32.png"))
    create_placeholder_icon(128, os.path.join(script_dir, "128x128.png"))
    create_placeholder_icon(256, os.path.join(script_dir, "128x128@2x.png"))
    create_placeholder_icon(512, os.path.join(script_dir, "512x512.png"))

    create_ico([16, 32, 48, 256], os.path.join(script_dir, "icon.ico"))

    print("\nPlaceholder icons generated successfully!")
    print("\nFor macOS .icns file, you need to run on macOS:")
    print("  mkdir icon.iconset")
    print("  python3 generate-placeholder-icons.py")
    print("  iconutil -c icns icon.iconset -o icon.icns")

if __name__ == "__main__":
    try:
        main()
    except ImportError:
        print("ERROR: PIL (Pillow) is required. Install with:")
        print("   pip install pillow")
        exit(1)
