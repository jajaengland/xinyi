"""Regenerate the procedural BMP bundled with Image viewer (stdlib only)."""
from pathlib import Path
import struct


def main():
    width,height=320,192;stride=(width*3+3)&~3
    data=bytearray(54+stride*height)
    struct.pack_into('<2sIHHI',data,0,b'BM',len(data),0,0,54)
    struct.pack_into('<IiiHHIIiiII',data,14,40,width,height,1,24,0,stride*height,2835,2835,0,0)
    for y in range(height):
        for x in range(width):
            red=x*255//(width-1);green=y*255//(height-1);blue=180 if (x//32+y//32)%2 else 48
            at=54+(height-1-y)*stride+x*3;data[at:at+3]=bytes((blue,green,red))
    output=Path(__file__).resolve().parents[1]/'assets/images/sample.bmp'
    output.parent.mkdir(parents=True,exist_ok=True);output.write_bytes(data)
    print(output)


if __name__=='__main__':main()
