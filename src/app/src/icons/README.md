# App Icons for Recall

This directory contains the application icons for all platforms.

## Required Icons

### For Production Builds

You need to generate the following icon files:

1. **32x32.png** - Windows taskbar icon (32x32 pixels)
2. **128x128.png** - macOS dock icon, Linux launcher (128x128 pixels)
3. **128x128@2x.png** - macOS retina display (256x256 pixels)
4. **icon.icns** - macOS bundle icon (multi-resolution)
5. **icon.ico** - Windows executable icon (multi-resolution)

### Recommended Sizes for Source Material

- Start with a high-resolution square image (at least 1024x1024 pixels)
- Simple, recognizable design that works at small sizes
- Use transparent background for better platform integration

## Icon Generation

### Option 1: Using Online Tools

1. Create your icon design (1024x1024 px, PNG with transparency)
2. Use tools like:
   - https://icon.kitchen/ (comprehensive)
   - https://favicon.io/ (quick generation)
   - https://realfavicongenerator.net/ (detailed options)

### Option 2: Using ImageMagick (Command Line)

```bash
# Install ImageMagick first
# macOS: brew install imagemagick
# Ubuntu: sudo apt-get install imagemagick
# Windows: Download from https://imagemagick.org/

# Generate PNG icons from source
convert icon-source.png -resize 32x32 32x32.png
convert icon-source.png -resize 128x128 128x128.png
convert icon-source.png -resize 256x256 128x128@2x.png
convert icon-source.png -resize 512x512 512x512.png

# Generate .ico (Windows)
convert 32x32.png 128x128.png 256x256.png icon.ico

# Generate .icns (macOS) - requires iconutil on macOS
mkdir icon.iconset
for size in 16 32 128 256 512; do
  convert icon-source.png -resize ${size}x${size} icon.iconset/icon_${size}x${size}.png
done
iconutil -c icns icon.iconset -o icon.icns
```

### Option 3: Using Tauri Icon Generator

```bash
# Install cargo-tauri if not already installed
cargo install tauri-cli

# Generate all icons from a single source image
npm run tauri icon path/to/your-icon.png
```

## Design Guidelines

### Visual Style

For a "Recall" knowledge management app, consider:
- **Brain/Neural Network** - Interconnected nodes representing knowledge
- **Book/Library** - Classic knowledge repository symbol
- **Vault/Lock** - Emphasizes privacy and security
- **Search/Magnifying Glass** - Highlights search functionality
- **Light Bulb** - Represents ideas and insights

### Technical Requirements

- **Format**: PNG with alpha transparency for source
- **Colors**: Use 2-3 colors maximum for clarity at small sizes
- **Contrast**: Ensure visibility on both light and dark backgrounds
- **Simplicity**: Avoid fine details that won't be visible at 32x32

## Current Status

⚠️ **ICONS MISSING** - Icons must be generated before building.

**Quick Start**: See `GENERATE_ICONS.md` for detailed instructions.

Before building:
1. Create or obtain a source icon (512x512+ PNG)
2. Run: `npm run tauri icon path/to/icon-source.png`
3. Verify all files are generated (see Required Icons above)
4. Test the build: `npm run tauri build`

## Testing Icons

After generating icons:

1. **macOS**: Check dock and Finder
2. **Windows**: Check taskbar, Start Menu, and .exe file
3. **Linux**: Check application menu and file manager

Build the app and verify:
```bash
npm run tauri:build
```

Then install the generated package and check all icon locations.
