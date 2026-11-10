"""Convert PNG/JPEG/etc. to a wallpaper BMP supported by Xinyi. Requires Pillow."""
import argparse
from pathlib import Path
from PIL import Image,ImageOps

def prepare(source,output):
    with Image.open(source) as image:
        if image.width*image.height>32*1024*1024:raise ValueError('Input exceeds the 32-megapixel conversion limit')
        image=ImageOps.exif_transpose(image).convert('RGBA');image.thumbnail((640,400),Image.Resampling.LANCZOS)
        background=Image.new('RGBA',image.size,(32,54,79,255));background.alpha_composite(image);image=background.convert('RGB')
        with Path(output).open('xb') as stream:image.save(stream,format='BMP')
    assert Path(output).stat().st_size<=768*1024

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('source',type=Path);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    try:prepare(args.source,args.output)
    except (OSError,ValueError) as error:parser.exit(1,str(error)+'\n')
    print(f'Prepared {args.output}: {args.output.stat().st_size} bytes. Copy into Xinyi and select it in Settings.')
if __name__=='__main__':main()
