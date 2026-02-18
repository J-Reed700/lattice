# Icon Generation Guide for Vault Desktop App

The Vault desktop app requires multiple icon files in different formats and sizes for proper builds across all platforms.

## Required Files

```
vault/desktop/src-tauri/icons/
├── 32x32.png          (32x32 pixels)
├── 128x128.png        (128x128 pixels)
├── 128x128@2x.png     (256x256 pixels)
├── icon.icns          (macOS icon bundle)
├── icon.ico           (Windows multi-size icon)
└── icon.png           (512x512+ source image)
```

## Option 1: Using Tauri CLI (Recommended)

This is the easiest and most reliable method.

```bash
# Navigate to the desktop directory
cd vault/desktop

# Create or obtain a source icon (PNG, 512x512 or larger, with transparency)
# Place it somewhere accessible, e.g., icon-source.png

# Generate all required icons automatically
npm run tauri icon path/to/icon-source.png

# This will create all required formats in src-tauri/icons/
```

**Benefits:**
- Generates all required formats automatically
- Ensures correct sizing and formats
- Handles platform-specific requirements (icns, ico)

## Option 2: Using icon.kitchen (Online Tool)

Great for non-developers or if Tauri CLI isn't working.

1. Visit https://icon.kitchen/
2. Upload your logo/icon design (PNG or SVG)
3. Select the **"Electron/Tauri"** preset
4. Download the generated icons package
5. Extract all files to `vault/desktop/src-tauri/icons/`

## Option 3: Manual Creation with ImageMagick

For developers who prefer command-line tools.

### Prerequisites

Install ImageMagick:
- **Windows**: Download from https://imagemagick.org/
- **macOS**: `brew install imagemagick`
- **Linux**: `sudo apt install imagemagick` or `sudo yum install ImageMagick`

### Generate Icons

```bash
cd vault/desktop/src-tauri/icons/

# Ensure you have a source icon.png (512x512 or larger)

# Generate PNG sizes
magick icon.png -resize 32x32 32x32.png
magick icon.png -resize 128x128 128x128.png
magick icon.png -resize 256x256 128x128@2x.png

# Generate Windows .ico (multi-resolution)
magick icon.png -define icon:auto-resize=256,128,64,48,32,16 icon.ico

# Generate macOS .icns
# Create iconset directory
mkdir icon.iconset

# Generate all required sizes for macOS
magick icon.png -resize 16x16 icon.iconset/icon_16x16.png
magick icon.png -resize 32x32 icon.iconset/icon_16x16@2x.png
magick icon.png -resize 32x32 icon.iconset/icon_32x32.png
magick icon.png -resize 64x64 icon.iconset/icon_32x32@2x.png
magick icon.png -resize 128x128 icon.iconset/icon_128x128.png
magick icon.png -resize 256x256 icon.iconset/icon_128x128@2x.png
magick icon.png -resize 256x256 icon.iconset/icon_256x256.png
magick icon.png -resize 512x512 icon.iconset/icon_256x256@2x.png
magick icon.png -resize 512x512 icon.iconset/icon_512x512.png
magick icon.png -resize 1024x1024 icon.iconset/icon_512x512@2x.png

# Convert to .icns (macOS only, requires iconutil)
iconutil -c icns icon.iconset

# Clean up
rm -rf icon.iconset
```

## Option 4: Quick Development Placeholders

For testing/development only. Creates simple colored squares.

### Using ImageMagick

```bash
cd vault/desktop/src-tauri/icons/

# Generate simple indigo squares
magick -size 32x32 xc:#4F46E5 32x32.png
magick -size 128x128 xc:#4F46E5 128x128.png
magick -size 256x256 xc:#4F46E5 128x128@2x.png
magick -size 512x512 xc:#4F46E5 icon.png

# Generate Windows .ico
magick icon.png -define icon:auto-resize=256,128,64,48,32,16 icon.ico

# Generate macOS .icns (basic version)
mkdir icon.iconset
magick -size 16x16 xc:#4F46E5 icon.iconset/icon_16x16.png
magick -size 32x32 xc:#4F46E5 icon.iconset/icon_16x16@2x.png
magick -size 32x32 xc:#4F46E5 icon.iconset/icon_32x32.png
magick -size 64x64 xc:#4F46E5 icon.iconset/icon_32x32@2x.png
magick -size 128x128 xc:#4F46E5 icon.iconset/icon_128x128.png
magick -size 256x256 xc:#4F46E5 icon.iconset/icon_128x128@2x.png
magick -size 256x256 xc:#4F46E5 icon.iconset/icon_256x256.png
magick -size 512x512 xc:#4F46E5 icon.iconset/icon_256x256@2x.png
magick -size 512x512 xc:#4F46E5 icon.iconset/icon_512x512.png
magick -size 1024x1024 xc:#4F46E5 icon.iconset/icon_512x512@2x.png
iconutil -c icns icon.iconset
rm -rf icon.iconset
```

### Using Python (Cross-platform)

```python
from PIL import Image

# Create simple colored square
color = (79, 70, 229)  # Indigo #4F46E5

sizes = {
    '32x32.png': (32, 32),
    '128x128.png': (128, 128),
    '128x128@2x.png': (256, 256),
    'icon.png': (512, 512)
}

for filename, size in sizes.items():
    img = Image.new('RGB', size, color)
    img.save(filename)

print("Basic PNG icons created. Use ImageMagick for .ico and .icns")
```

## Design Guidelines

When creating your custom icon:

### Dimensions
- **Minimum source size**: 512x512px
- **Recommended**: 1024x1024px for best quality
- **Format**: PNG with transparency

### Visual Style
- **Style**: Modern, minimalist
- **Primary color**: #4F46E5 (Indigo)
- **Secondary color**: #8B5CF6 (Purple)
- **Concept ideas**:
  - Brain/neural network (memory/cognition)
  - Vault/lock (security/privacy)
  - Cloud with lock (secure storage)
  - Abstract geometric pattern

### Technical Requirements
- **Background**: Transparent (alpha channel)
- **Padding**: Leave 10% margin on all sides
- **Shape**: Works well at small sizes (16x16)
- **Avoid**: Fine details that disappear when scaled down

## Verification

After generating icons, verify all files exist:

```bash
cd vault/desktop/src-tauri/icons/
ls -la

# You should see:
# 32x32.png
# 128x128.png
# 128x128@2x.png
# icon.icns
# icon.ico
# icon.png
```

Test the build:

```bash
cd vault/desktop
npm run tauri build
```

## Troubleshooting

### "npm run tauri icon" fails
- Ensure you're in `vault/desktop` directory
- Check that Tauri CLI is installed: `npm install`
- Verify source image is PNG format

### .icns generation fails on Windows
- Use icon.kitchen instead
- Or generate .icns on macOS/Linux and commit to repo

### Build still fails
- Check `tauri.conf.json` icon paths match generated files
- Ensure all files are in `vault/desktop/src-tauri/icons/`
- Verify file permissions (readable)

## Resources

- [Tauri Icon Documentation](https://tauri.app/v1/guides/features/icons)
- [icon.kitchen](https://icon.kitchen/)
- [ImageMagick Documentation](https://imagemagick.org/script/convert.php)
- [macOS iconutil](https://developer.apple.com/library/archive/documentation/GraphicsAnimation/Conceptual/HighResolutionOSX/Optimizing/Optimizing.html)
