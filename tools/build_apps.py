"""Compile standard applications as separate filesystem files."""
import argparse
import json
from pathlib import Path
import re
import shutil
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
        (output/"apps"/name).mkdir(exist_ok=True)
        subprocess.run([sys.executable,str(ROOT/"tools/build_program.py"),str(source),
                        "--output",str(output/f"apps/{name}/{name}{extension}"),"--force","--format",format],check=True)
        size=(output/f"apps/{name}/{name}{extension}").stat().st_size
        if not 0<size<=(1024*1024 if format=="elf" else 65536):raise ValueError(f"Invalid image size for {name}: {size}")
        manifest=ROOT/"programs/manifests"/(name+".app")
        content=manifest.read_bytes()
        if not 0<len(content)<=512 or content.splitlines()[0]!=b"XINYI-APP 1":raise ValueError(f"Invalid manifest: {manifest}")
        pairs=[line.split("=",1) for line in content.decode('ascii').splitlines()[1:] if line and not line.startswith('#')]
        properties=dict(pairs)
        if len(pairs)!=6 or set(properties)!={'exec','title','description','icon','access','handles'}:raise ValueError(f"Duplicate, missing or unknown manifest fields: {manifest}")
        if not properties['title'].strip() or len(properties['title'])>32 or len(properties['description'])>64:raise ValueError(f"Invalid manifest text: {manifest}")
        if properties['access'] not in ('trusted','read-only','media','isolated') or properties['icon'] not in ('folder','terminal','settings','document','audio','game','font','application'):raise ValueError(f"Invalid manifest policy/icon: {manifest}")
        types=properties['handles'].lower().split(',') if properties['handles'] else []
        if len(types)>8 or len(set(types))!=len(types) or any(t in ('bin','elf') or not re.fullmatch(r'[a-z0-9]{1,3}|-',t) for t in types):raise ValueError(f"Invalid file associations: {manifest}")
        if properties.get('exec','').upper()!=name.upper()+extension.upper():raise ValueError(f"Manifest executable mismatch: {manifest}")
        shutil.copyfile(manifest,output/f"apps/{name}/{name}.app")
        entries.append({"directory":name.upper(),"name":name,"title":properties['title'],"description":properties['description'],"icon":properties['icon'],"access":properties['access'],"file":name.upper()+extension.upper(),"manifest":name.upper()+".APP"})
    if not entries:raise ValueError("The application manifest must not be empty")
    (output/"applications.json").write_text(json.dumps(entries,indent=2))



if __name__=="__main__":main()
