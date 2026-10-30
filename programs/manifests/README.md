# Application manifests

Each `/APPS/NAME.BIN` or `/APPS/NAME.ELF` can have a sibling `/APPS/NAME.APP`.
These source files are copied into release images by `tools/build_apps.py` and
`tools/seed_apps.py`. The OS installer copies the sidecars with the executables.
Changing metadata takes effect when Apps refreshes; the loader rereads permission
restrictions on every launch, including launches from Terminal/the physical shell.

Example (printable ASCII, LF or CRLF, at most 512 bytes):

```
XINYI-APP 1
exec=PLAYER.ELF
title=Player
description=WAV and voice playback
icon=audio
access=media
handles=wav
```

All six keys are required exactly once. Unknown keys, duplicates, unsupported
versions, invalid values and executable mismatches are rejected. `exec` must
match the sibling executable; it cannot redirect to another file or path.

- `title`: 1–32 printable ASCII bytes, not all spaces.
- `description`: 0–64 printable ASCII bytes.
- `icon`: folder, terminal, settings, document, audio, game, font, application.
- `access`: trusted, read-only, media, isolated. This intersects existing kernel
  filename defaults and the requested launch mode; metadata cannot elevate either.
- `handles`: empty, or up to eight distinct comma-separated extensions of 1–3
  alphanumeric characters. `-` means no extension. Matching ignores ASCII case.
  BIN/ELF are reserved for explicit executable opening.

Missing metadata retains legacy executable discovery with its filename and a
generic icon. Present but invalid metadata blocks that executable's normal launch;
the launcher uses fallback display text so the file remains discoverable.

Files scans bounded installed metadata for a matching handler. There is no
automatic fallback for unknown types and no silent winner if two handlers claim
the same type. Launch an app explicitly to choose manually. The first implementation
supports up to 16 executable candidates and 64 directory entries for associations.

These manifests are local metadata, not signed packages or proof of an executable's
identity. Installation management, default-app selection and permission prompts
are later stages of the application system.

Installed bundles keep metadata beside the executable inside `/APPS/ID`: for example `/APPS/NOTEPAD/NOTEPAD.APP` and `/APPS/NOTEPAD/NOTEPAD.BIN`. Use the same stem as the bundle folder. Files installs a selected source folder after confirmation and removes selected installed folders from `/APPS`. Assets may use subfolders within the bounded tree limits.
