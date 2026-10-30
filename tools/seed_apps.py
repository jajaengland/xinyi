"""Populate /APPS in a closed, private FAT16 release/test image."""
import json
from pathlib import Path
import struct
from folders_qemu import inspect_tree


def seed_apps(image,package,root_name="APPS"):
    assert root_name.isalpha() and 1<=len(root_name)<=8
    root_name=root_name.upper()
    image=Path(image);package=Path(package)
    data=bytearray(image.read_bytes());start=struct.unpack_from("<I",data,454)[0]
    before,_=inspect_tree(image,start)
    boot=start*512;spc=data[boot+13];fats=data[boot+16]
    reserved,entries,spf=(struct.unpack_from("<H",data,boot+offset)[0] for offset in (14,17,22))
    total=struct.unpack_from("<H",data,boot+19)[0] or struct.unpack_from("<I",data,boot+32)[0]
    fat=start+reserved;root=fat+fats*spf;first=root+(entries*32+511)//512
    count=(total-(first-start))//spc
    def word(offset):return struct.unpack_from("<H",data,offset)[0]
    def fat_value(cluster):return word(fat*512+cluster*2)
    def set_fat(cluster,value):
        for copy in range(fats):struct.pack_into("<H",data,(fat+copy*spf)*512+cluster*2,value)
    def sector(cluster):return (first+(cluster-2)*spc)*512
    def allocate():
        for cluster in range(2,count+2):
            if fat_value(cluster)==0:
                set_fat(cluster,0xffff);offset=sector(cluster);data[offset:offset+spc*512]=bytes(spc*512);return cluster
        raise RuntimeError("No free FAT16 clusters")
    def chain(cluster):
        result=[]
        while cluster:
            assert cluster not in result and 2<=cluster<count+2
            result.append(cluster);cluster=fat_value(cluster)
            if cluster>=0xfff8:break
        return result
    def slots(directory):
        if directory==0:return [root*512+i*32 for i in range(entries)]
        return [sector(cluster)+i*32 for cluster in chain(directory) for i in range(spc*16)]
    def find(directory,name):
        return next((offset for offset in slots(directory) if data[offset:offset+11]==name),None)
    def create(directory,name,attr,cluster,size):
        free=next((offset for offset in slots(directory) if data[offset] in (0,0xe5)),None)
        if free is None:
            if directory==0:raise RuntimeError("Root directory full")
            tail=chain(directory)[-1];extra=allocate();set_fat(tail,extra);free=sector(extra)
        entry=bytearray(32);entry[:11]=name;entry[11]=attr
        for offset in (16,18,24):struct.pack_into("<H",entry,offset,0x21)
        struct.pack_into("<H",entry,26,cluster);struct.pack_into("<I",entry,28,size)
        data[free:free+32]=entry
    root_encoded=root_name.ljust(11).encode()
    apps=find(0,root_encoded)
    if apps is None:
        directory=allocate();create(0,root_encoded,0x10,directory,0)
        create(directory,b".          ",0x10,directory,0);create(directory,b"..         ",0x10,0,0)
    else:
        assert data[apps+11]&0x10;directory=word(apps+26)
    expected={}
    artifacts=[]
    for app in json.loads((package/"applications.json").read_text()):
        artifacts.append(app)
        if app.get('manifest'):artifacts.append({'file':app['manifest'],'directory':app.get('directory','')})
    for app in artifacts:
        folder=app.get('directory','');parent=directory
        if folder:
            assert 1<=len(folder)<=8 and folder[0].isalpha() and all(c.isalnum() or c=='_' for c in folder)
            encoded=folder.upper().ljust(11).encode();entry=find(directory,encoded)
            if entry is None:
                parent=allocate();create(directory,encoded,0x10,parent,0)
                create(parent,b".          ",0x10,parent,0);create(parent,b"..         ",0x10,directory,0)
            else:
                assert data[entry+11]&0x10;parent=word(entry+26)
        payload=(package/"apps"/folder.lower()/app["file"].lower()).read_bytes();name=app["file"]
        key=root_name+"/"+(folder.upper()+"/" if folder else "")+name
        assert 0<len(payload)<=(1024*1024 if payload.startswith(b"\x7fELF") else 65536)
        expected[key]=payload
        base,extension=name.split(".");encoded=(base.ljust(8)+extension.ljust(3)).encode()
        if find(parent,encoded) is not None:
            assert before[key]==payload,"Refusing to replace existing application"
            continue
        clusters=[]
        for offset in range(0,len(payload),spc*512):
            cluster=allocate()
            if clusters:set_fat(clusters[-1],cluster)
            clusters.append(cluster);chunk=payload[offset:offset+spc*512];at=sector(cluster);data[at:at+len(chunk)]=chunk
        create(parent,encoded,0x20,clusters[0],len(payload))
    with image.open("r+b") as stream:stream.write(data)
    files,_=inspect_tree(image,start)
    assert all(files[path]==content for path,content in before.items())
    assert all(files[path]==content for path,content in expected.items())
    return expected
