# ⚠️ ICONS REQUIRED BEFORE BUILD

## Problem

The application icons are currently **MISSING**. The Tauri build will **FAIL** without them.

## Required Files

The following files must exist in this directory:

- [ ] `32x32.png` (32x32 pixels)
- [ ] `128x128.png` (128x128 pixels)
- [ ] `128x128@2x.png` (256x256 pixels)
- [ ] `icon.icns` (macOS bundle icon)
- [ ] `icon.ico` (Windows multi-size icon)
- [ ] `icon.png` (512x512+ source image)

## Quick Fix (Recommended)

```bash
# 1. Get or create a 512x512 PNG icon
# 2. Navigate to the desktop directory
cd vault/desktop

# 3. Generate all required icons automatically
npm run tauri icon path/to/your-icon.png

# 4. Verify icons were created
ls src-tauri/icons/

# 5. Build the app
npm run tauri build
```

## Need Help?

See **`GENERATE_ICONS.md`** in this directory for:
- Step-by-step instructions for all methods
- Online tools (icon.kitchen)
- Manual generation with ImageMagick
- Quick placeholder generation for testing
- Design guidelines

## Alternative: Quick Placeholder (Testing Only)

If you just need to test the build and don't care about the icon:

```bash
cd vault/desktop/src-tauri/icons/

# Using ImageMagick (must be installed)
magick -size 32x32 xc:#4F46E5 32x32.png
magick -size 128x128 xc:#4F46E5 128x128.png
magick -size 256x256 xc:#4F46E5 128x128@2x.png
magick -size 512x512 xc:#4F46E5 icon.png
magick icon.png -define icon:auto-resize=256,128,64,48,32,16 icon.ico

# For .icns (macOS), use Tauri CLI or icon.kitchen
```

## Status

❌ **Icons not generated yet**

Once icons are generated, delete this file and proceed with building.
