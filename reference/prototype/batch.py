import numpy as np, dng, analyze, glob, os, json, struct, sys

def meta(fn):
    f=open(fn,'rb');h=f.read(4);end='<' if h[:2]==b'II' else '>';off=struct.unpack(end+'I',f.read(4))[0]
    def ifd(o):
        f.seek(o);n=struct.unpack(end+'H',f.read(2))[0];r={}
        for _ in range(n):
            raw=f.read(12);t,ty,c=struct.unpack(end+'HHI',raw[:8]);r[t]=(ty,c,raw[8:12])
        return r
    def v(t):
        ty,c,vr=t;sz={1:1,2:1,3:2,4:4,5:8}[ty]*c
        if sz<=4:d=vr[:sz]
        else:o=struct.unpack(end+'I',vr)[0];f.seek(o);d=f.read(sz)
        if ty==5:vv=struct.unpack(end+'%dI'%(c*2),d);return vv[0]/vv[1]
        return struct.unpack(end+'%d'%c+{1:'B',3:'H',4:'I'}[ty],d)[0]
    i0=ifd(off);ex=ifd(int(struct.unpack(end+'I',i0[34665][2])[0]))
    return v(ex[33437]),v(ex[34855])

folders=sys.argv[1:]
rows=[]
for d in folders:
    for p in sorted(glob.glob(f"{d}/*.DNG")):
        F,iso=meta(p)
        m,g,n=analyze.measure(p)
        rec=dict(file=os.path.basename(p),F=F,iso=iso,
                 zones={k:(round(m[k]['acut'],3),round(m[k]['contrast'],3)) for k in m})
        rows.append(rec)
        z=rec['zones']
        print(f"{rec['file'][:-4]:9s} f/{F:<4g} | TL{z['TL'][0]:.2f} TR{z['TR'][0]:.2f} C{z['C'][0]:.2f} BL{z['BL'][0]:.2f} BR{z['BR'][0]:.2f}  (contrast min {min(z[k][1] for k in ['TL','TR','BL','BR']):.2f})")
json.dump(rows,open('/tmp/batch_'+os.path.basename(folders[0])+'.json','w'))
print("saved",len(rows),"frames")
