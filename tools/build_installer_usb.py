"""Create a BIOS USB image that opens the graphical Xinyi installer."""
import argparse
from pathlib import Path
import shutil
import tempfile

from install_qemu import verify_payload
from seed_apps import seed_apps


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output already exists")
    release = args.release.resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=args.output.parent) as directory:
        image = Path(directory) / "installer.img"
        shutil.copyfile(release / "xinyi.img", image)
        verify_payload(image, release / "rust-os")
        seed_apps(image, release, installer=True)
        with image.open("rb") as source, args.output.open("xb") as destination:
            shutil.copyfileobj(source, destination)
    print(args.output.resolve())


if __name__ == "__main__":
    main()
