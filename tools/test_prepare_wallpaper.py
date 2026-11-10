import tempfile,unittest
from pathlib import Path
from PIL import Image
from tools.prepare_wallpaper import prepare
class WallpaperConverterTests(unittest.TestCase):
 def test_photo_downsizes_to_supported_bmp(self):
  with tempfile.TemporaryDirectory() as temp:
   source=Path(temp)/'photo.png';target=Path(temp)/'WALL.BMP';Image.new('RGB',(1600,900),(120,40,70)).save(source);prepare(source,target)
   with Image.open(target) as result:self.assertEqual(result.format,'BMP');self.assertEqual(result.mode,'RGB');self.assertLessEqual(result.width,640);self.assertLessEqual(result.height,400)
   self.assertLessEqual(target.stat().st_size,768*1024)
 def test_alpha_and_existing_output(self):
  with tempfile.TemporaryDirectory() as temp:
   source=Path(temp)/'alpha.png';target=Path(temp)/'WALL.BMP';Image.new('RGBA',(2,2),(0,0,0,0)).save(source);prepare(source,target)
   with Image.open(target) as result:self.assertEqual(result.getpixel((0,0)),(32,54,79))
   original=target.read_bytes()
   with self.assertRaises(FileExistsError):prepare(source,target)
   self.assertEqual(target.read_bytes(),original)
if __name__=='__main__':unittest.main()
