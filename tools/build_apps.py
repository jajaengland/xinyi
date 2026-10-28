"""Compile standard applications as separate filesystem files."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT=Path(__file__).resolve().parents[1]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir",type=Path,required=True)
    args=parser.parse_args()
    output=args.output_dir.resolve();(output/"apps").mkdir(parents=True,exist_ok=True)
    entries=[];names=set()
    for line in (ROOT/"programs/apps.txt").read_text().splitlines():
        if not line.strip() or line.lstrip().startswith("#"):continue
        fields=line.split("|");name,source,description=fields[:3];format=fields[3] if len(fields)>=4 else "flat"
        if len(fields) not in (3,4,5) or format not in ("flat","elf"):raise ValueError("Invalid application format")
        extension="."+fields[4] if len(fields)==5 else (".elf" if format=="elf" else ".bin")
        if extension not in (".elf",".bin"):raise ValueError("Invalid application extension")
        if not re.fullmatch(r"[a-z][a-z0-9_]{0,7}",name) or name in names or name=="demo":
            raise ValueError(f"Invalid or duplicate application name: {name}")
        names.add(name)
        source=(ROOT/source).resolve();source.relative_to(ROOT/"programs")
        subprocess.run([sys.executable,str(ROOT/"tools/build_program.py"),str(source),
                        "--output",str(output/f"apps/{name}{extension}"),"--force","--format",format],check=True)
        size=(output/f"apps/{name}{extension}").stat().st_size
        if not 0<size<=65536:raise ValueError(f"Invalid image size for {name}: {size}")
        entries.append({"name":name,"description":description,"file":name.upper()+extension.upper()})
    if not entries:raise ValueError("The application manifest must not be empty")
    (output/"applications.json").write_text(json.dumps(entries,indent=2))



if __name__=="__main__":main()
