# Image wallpapers

Open **Settings**, then use the **Wallpaper** panel:

1. **Choose image** opens the shared file picker. A fresh image offers `/APPS/SETTINGS/WALL.BMP`, a bundled sample; choose another file or edit the path to use your own BMP.
2. Click **Mode** to cycle through **Fit**, **Fill** and **Center**.
3. **Apply** previews the change for this desktop session. **Save** also persists the selected path and mode on the current drive.
4. **Use colour**, followed by Apply or Save, restores the normal coloured desktop. Defaults also selects the normal background.

Fit scales the entire image while preserving its aspect ratio; uncovered areas use the desktop colour. Fill scales and crops the centre to cover the work area. Center uses native pixels, cropping oversized images symmetrically. The taskbar remains separate. Labels receive dark backplates over images for readability.

## Image files

Xinyi accepts uncompressed Windows BMP files with 24-bit or 32-bit RGB pixels, at most **768 KiB** and **2048 pixels per dimension**. BMP alpha is ignored. Rendering currently uses nearest-neighbour scaling; the converted result is cached in kernel-owned memory rather than decoded during desktop redraws.

Convert a PNG, JPEG or another Pillow-supported image on the host:

```powershell
python tools/prepare_wallpaper.py "C:/Pictures/photo.jpg" --output target/PHOTO.BMP
```

The converter requires Pillow, handles EXIF orientation, shrinks images to fit 640x400 with Lanczos filtering, and flattens transparency over the default desktop colour. It refuses an existing output file. Copy the resulting BMP onto the Xinyi drive before selecting it. The converter is a host tool; PNG/JPEG decoding has not been added to the kernel.

## Persistence and failure behavior

The image stays at its selected path. `/WALL.CFG` stores the choice, not a duplicate image. Moving/deleting the source leaves the current cached wallpaper visible until it is replaced or the desktop is restarted. If the file is missing, damaged or too large on restart, Xinyi falls back to the coloured desktop without overwriting the preference. Settings reports an unavailable saved wallpaper and lets you choose a replacement or save Use colour.

Failed Apply/Save requests retain the previous active wallpaper. Preferences belong to the current drive; a changed volume invalidates the old cached image. Settings rejects a stale selection. Changes are read again at desktop startup, including after a display-resolution change. Leaving the desktop releases its cached image.

The on-disk LF ASCII record is `XINYI-WALL 1`, a line containing mode 0/1/2 (Fit/Fill/Center), and an absolute 8.3 path line. An empty path means the normal background. Bounds and format are checked before loading.
