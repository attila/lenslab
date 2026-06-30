import numpy as np, dng, glob, os
from PIL import Image, ImageDraw
files=sorted(glob.glob("/sessions/cool-practical-dijkstra/mnt/2026/*/I*.DNG")+
             glob.glob("/sessions/cool-practical-dijkstra/mnt/2026/*/_*.DNG"))
# keep only 25mm
sel=[]
for f in files:
    n=dng.info(f)
    sel.append(f)
tw,th=260,195
cols=8
import math
def thumb(f):
    a,n=dng.decode(f)
    B=a[0::2,0::2];G1=a[0::2,1::2];G2=a[1::2,0::2];R=a[1::2,1::2]
    # CFA may vary; detect from pattern
    pat=n['cfa_pattern']
    G=(G1+G2)/2
    rgb=np.stack([R,G,B],-1)/n['white']
    rgb[...,0]*=1.6;rgb[...,2]*=1.5;rgb=np.clip(rgb,0,1)
    im=Image.fromarray((rgb**(1/2.2)*255).astype('uint8')).resize((tw,th))
    os.remove(f"/tmp/raws/{os.path.splitext(os.path.basename(f))[0]}.raw")
    return im
# only 25mm frames
import struct
def fnum(f):
    nf=dng.info(f); return nf
meta={}
exec(open('/tmp/meta.py').read()) if os.path.exists('/tmp/meta.py') else None
# get aperture+focal via meta parse inline
def getmeta(fn):
    import struct
    f=open(fn,'rb');h=f.read(4);end='<' if h[:2]==b'II' else '>'
    off=struct.unpack(end+'I',f.read(4))[0]
    def ifd(o):
        f.seek(o);n=struct.unpack(end+'H',f.read(2))[0];r={}
        for _ in range(n):
            raw=f.read(12);t,ty,c=struct.unpack(end+'HHI',raw[:8]);r[t]=(ty,c,raw[8:12])
        return r
    def v(t):
        ty,c,vr=t;sz={3:2,4:4,5:8}[ty]*c
        if sz<=4:d=vr[:sz]
        else:o=struct.unpack(end+'I',vr)[0];f.seek(o);d=f.read(sz)
        if ty==5:vv=struct.unpack(end+'%dI'%(c*2),d);return vv[0]/vv[1]
        return struct.unpack(end+'%dH'%c,d)[0]
    i0=ifd(off);ex=ifd(int(struct.unpack(end+'I',i0[34665][2])[0]))
    return v(ex[33437]), v(ex[37386])
frames=[]
for f in sel:
    F,fl=getmeta(f)
    if abs(fl-25)<1: frames.append((f,F))
print("25mm frames:",len(frames))
N=len(frames);rows=math.ceil(N/cols)
sheet=Image.new('RGB',(cols*tw,rows*(th+16)),(20,20,20))
d=ImageDraw.Draw(sheet)
for i,(f,F) in enumerate(frames):
    im=thumb(f);r,c=divmod(i,cols)
    sheet.paste(im,(c*tw,r*(th+16)))
    d.text((c*tw+3,r*(th+16)+th+1),f"{os.path.basename(f)[:-4]} f/{F:g}",fill=(230,230,230))
sheet.save("contact_25mm.png")
print("saved",sheet.size)
