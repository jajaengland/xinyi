"""Build a bounded .XAP offline package from an ID-named application bundle folder."""
import argparse
from pathlib import Path
import re
import struct
import zlib
MAX_BYTES=768*1024
NAME=re.compile(r'[A-Z0-9_-]{1,8}(?:\.[A-Z0-9_-]{1,3})?\Z')
ID=re.compile(r'[A-Z][A-Z0-9_]{0,7}\Z')
def build(folder):
    folder=Path(folder)
    if folder.is_symlink() or folder.is_junction() or not folder.is_dir():raise ValueError('Source must be an ordinary bundle directory')
    identity=folder.name.upper()
    if not ID.fullmatch(identity):raise ValueError('Bundle folder must have a valid application ID')
    records=[];seen=set();payload=bytearray()
    def visit(parent,parts):
        for path in sorted(parent.iterdir(),key=lambda p:p.name.upper()):
            if path.is_symlink() or path.is_junction():raise ValueError('Links and junctions are not allowed')
            name=path.name.upper();relative=parts+[name];text='/'.join(relative)
            if not NAME.fullmatch(name) or len(relative)>6 or len(text)>77:raise ValueError('Invalid or overly deep 8.3 path: '+text)
            if text in seen:raise ValueError('Case-insensitive path collision: '+text)
            seen.add(text)
            directory=path.is_dir()
            if not directory and not path.is_file():raise ValueError('Only files and directories are allowed')
            if len(records)==63:raise ValueError('Packages allow at most 63 entries')
            if not directory and path.stat().st_size>MAX_BYTES:raise ValueError('File exceeds package size limit')
            data=b'' if directory else path.read_bytes()
            records.append((text,directory,data));encoded=text.encode('ascii')
            payload.extend(struct.pack('<BBHI',len(encoded),int(directory),0,len(data))+encoded+data)
            if len(payload)+24>MAX_BYTES:raise ValueError('Package exceeds the initial 768 KiB limit')
            if directory:visit(path,relative)
    visit(folder,[])
    files={name:data for name,directory,data in records if not directory}
    executables=[name for name in (identity+'.BIN',identity+'.ELF') if name in files]
    if len(executables)!=1 or not files[executables[0]]:raise ValueError('Provide exactly one nonempty ID.BIN or ID.ELF')
    manifest=files.get(identity+'.APP',b'')
    if not 1<=len(manifest)<=512:raise ValueError('Missing or oversized ID.APP manifest')
    try:lines=manifest.decode('ascii').splitlines()
    except UnicodeDecodeError:raise ValueError('Manifest must be ASCII')
    if not lines or lines[0]!='XINYI-APP 1' or ('exec='+executables[0]) not in lines:raise ValueError('Manifest header or executable does not match')
    header=b'XINYIPK1'+identity.encode().ljust(8,b'\0')+struct.pack('<HH',len(records),0)
    return header+struct.pack('<I',zlib.crc32(header+payload))+payload

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('folder',type=Path);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    try:
        data=build(args.folder)
        with args.output.open('xb') as stream:stream.write(data)
    except (OSError,ValueError) as error:parser.exit(1,str(error)+'\n')
    print(f'Built {args.output}: {len(data)} bytes. Install/Update through Applications; publishers are not authenticated.')
if __name__=='__main__':main()
