# Makes crates/gezik-batch/tests/data/fields/ (spec 13d test fixtures).
import io, os, subprocess, sys, zipfile
from PIL import Image
from PIL.TiffImagePlugin import IFDRational as R

out = sys.argv[1]
os.makedirs(out, exist_ok=True)


def ff(*args):
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', *args], check=True)


# photo.jpg: a camera's EXIF, turned (orientation 6)
im = Image.new('RGB', (32, 24), (200, 30, 30))
ex = Image.Exif()
ex[0x010F] = 'Apple'
ex[0x0110] = 'iPhone 15 Pro'
ex[0x0112] = 6
e = ex.get_ifd(0x8769)
e[0x829A] = R(1, 250)
e[0x829D] = R(9, 5)
e[0x8827] = 100
e[0x920A] = R(686, 100)
e[0x9003] = '2024:07:01 09:30:00'
e[0xA434] = 'iPhone 15 Pro back camera 6.86mm f/1.78'
g = ex.get_ifd(0x8825)
g[1] = 'N'
g[2] = (R(41, 1), R(0, 1), R(2952, 100))
g[3] = 'E'
g[4] = (R(28, 1), R(58, 1), R(4224, 100))
im.save(os.path.join(out, 'photo.jpg'), exif=ex, quality=60)
Image.new('RGB', (16, 16), (20, 120, 220)).save('cover.png')

meta = ['-metadata', 'title=Ağıt Şarkısı', '-metadata', 'artist=Çiğdem Öztürk', '-metadata', 'album=Gölge',
        '-metadata', 'album_artist=Öztürk Ailesi', '-metadata', 'genre=Halk', '-metadata', 'composer=Anonim',
        '-metadata', 'track=3/12', '-metadata', 'disc=1/2', '-metadata', 'date=2024']
sine = ['-f', 'lavfi', '-i', 'sine=frequency=440:duration=2']
cov = ['-i', 'cover.png', '-map', '0', '-map', '1', '-c:v', 'png', '-disposition:v', 'attached_pic']
ff(*sine, '-ac', '1', '-c:a', 'libmp3lame', '-b:a', '32k', '-id3v2_version', '3', *meta, os.path.join(out, 'id3v23.mp3'))
ff(*sine, *cov, '-ac', '1', '-c:a', 'libmp3lame', '-b:a', '32k', '-id3v2_version', '4', *meta,
   os.path.join(out, 'id3v24.mp3'))
ff(*sine, '-ac', '1', '-c:a', 'libmp3lame', '-b:a', '32k', '-id3v2_version', '0', '-write_xing', '0',
   os.path.join(out, 'id3v1.mp3'))
# ID3v1.1 by hand (ffmpeg leaves it out without ID3v2): Latin-1, track 7 after a NUL.
pad = lambda t, n: t.encode('cp1252').ljust(n, bytes(1))
tag = b'TAG' + pad('Gölge', 30) + pad('Öztürk', 30) + pad('Ünlü', 30) + b'1999' + pad('', 28) + bytes([0, 7, 255])
open(os.path.join(out, 'id3v1.mp3'), 'ab').write(tag)
ff(*sine, *cov, '-ac', '1', '-ar', '8000', '-c:a', 'flac', *meta, os.path.join(out, 'tags.flac'))
ff(*sine, '-ac', '1', '-c:a', 'libopus', '-b:a', '12k', *meta, os.path.join(out, 'tags.opus'))
ff(*sine, '-ac', '1', '-ar', '8000', '-c:a', 'libvorbis', '-q:a', '0', *meta, os.path.join(out, 'tags.ogg'))
ff(*sine, *cov, '-ac', '1', '-c:a', 'aac', '-b:a', '24k', *meta, os.path.join(out, 'tags.m4a'))
vid = ['-f', 'lavfi', '-i', 'testsrc=size=64x48:rate=10:duration=2', '-f', 'lavfi', '-i', 'sine=duration=2']
ff(*vid, '-c:v', 'libx264', '-crf', '45', '-c:a', 'aac', '-b:a', '16k', '-ac', '1', '-metadata', 'title=Deneme',
   os.path.join(out, 'moov-last.mp4'))
ff(*vid, '-c:v', 'libx264', '-crf', '45', '-c:a', 'libopus', '-b:a', '8k', '-ac', '1', '-metadata', 'title=Deneme',
   os.path.join(out, 'video.mkv'))
ff(*vid, '-c:v', 'mpeg4', '-q:v', '31', '-c:a', 'libmp3lame', '-b:a', '8k', '-ac', '1', '-ar', '8000',
   os.path.join(out, 'video.avi'))
ff('-f', 'lavfi', '-i', 'sine=duration=0.5', '-ac', '1', '-ar', '8000', '-c:a', 'pcm_s16le', '-metadata', 'title=Dalga',
   '-metadata', 'artist=Öztürk', os.path.join(out, 'tags.wav'))
ff('-f', 'lavfi', '-i', 'sine=duration=0.25', '-ac', '2', '-ar', '8000', '-c:a', 'pcm_s16be',
   os.path.join(out, 'plain.aiff'))

core = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
        'xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Yıllık Rapor &amp; Özet</dc:title>'
        '<dc:subject>Bütçe</dc:subject><dc:creator>Ayşe Yılmaz</dc:creator></cp:coreProperties>')
app = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
       '<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties">'
       '<Pages>12</Pages><Words>3456</Words><Application>Microsoft Office Word</Application></Properties>')
with zipfile.ZipFile(os.path.join(out, 'report.docx'), 'w', zipfile.ZIP_DEFLATED) as z:
    z.writestr('[Content_Types].xml', '<Types/>')
    z.writestr('docProps/core.xml', core)
    z.writestr('docProps/app.xml', app)
    z.writestr('word/document.xml', '<w:document/>')
meta_xml = ('<?xml version="1.0" encoding="UTF-8"?>\n'
            '<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
            'xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" '
            'office:version="1.3"><office:meta><meta:initial-creator>Mehmet Kaya</meta:initial-creator>'
            '<dc:title>Toplantı Notları</dc:title><dc:subject>Proje</dc:subject>'
            '<meta:document-statistic meta:table-count="0" meta:page-count="3" meta:word-count="512"/>'
            '</office:meta></office:document-meta>')
with zipfile.ZipFile(os.path.join(out, 'notes.odt'), 'w') as z:
    z.writestr('mimetype', 'application/vnd.oasis.opendocument.text')
    z.writestr('meta.xml', meta_xml)
    z.writestr('content.xml', '<office:document-content/>')

# A PDF whose title is UTF-16BE with a mark; the author a literal string with escapes.
title = 'Yıllık Özet'.encode('utf-16-be')
objs = [b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>',
        b'<< /Title <FEFF' + title.hex().upper().encode() + b'> /Author (Ay\\(s\\)e \\101) /Subject (B\\374t\\347e) >>']
pdf = io.BytesIO()
pdf.write(b'%PDF-1.4\n')
offs = []
for i, o in enumerate(objs, 1):
    offs.append(pdf.tell())
    pdf.write(b'%d 0 obj\n' % i + o + b'\nendobj\n')
x = pdf.tell()
pdf.write(b'xref\n0 %d\n0000000000 65535 f \n' % (len(objs) + 1))
for o in offs:
    pdf.write(b'%010d 00000 n \n' % o)
pdf.write(b'trailer\n<< /Size %d /Root 1 0 R /Info 5 0 R >>\nstartxref\n%d\n%%%%EOF\n' % (len(objs) + 1, x))
open(os.path.join(out, 'info.pdf'), 'wb').write(pdf.getvalue())
