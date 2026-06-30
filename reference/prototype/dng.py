"""DNG helpers: parse tags, extract raw lossless-JPEG strip, decode via ./ljpeg, return CFA ndarray."""
import struct, subprocess, os, numpy as np
TYPESIZE={1:1,2:1,3:2,4:4,5:8,6:1,7:1,8:2,9:4,10:8,11:4,12:8}

def _read(fn):
    f=open(fn,'rb'); h=f.read(4); end='<' if h[:2]==b'II' else '>'
    off=struct.unpack(end+'I',f.read(4))[0]
    def ifd(off):
        f.seek(off); n=struct.unpack(end+'H',f.read(2))[0]; r={}
        for _ in range(n):
            raw=f.read(12); tag,typ,cnt=struct.unpack(end+'HHI',raw[:8]); r[tag]=(typ,cnt,raw[8:12])
        return r
    def val(t):
        typ,cnt,vraw=t; size=TYPESIZE.get(typ,1)*cnt
        if size<=4: data=vraw[:size]
        else:
            o=struct.unpack(end+'I',vraw)[0]; f.seek(o); data=f.read(size)
        if typ==2: return data.split(b'\x00')[0].decode('latin1')
        if typ==3: return list(struct.unpack(end+'%dH'%cnt,data))
        if typ==4: return list(struct.unpack(end+'%dI'%cnt,data))
        if typ in(5,10):
            v=struct.unpack(end+('%dI'%(cnt*2) if typ==5 else '%di'%(cnt*2)),data)
            return [(v[i],v[i+1]) for i in range(0,len(v),2)]
        return data
    return f,ifd,val,off

def info(fn):
    f,ifd,val,off=_read(fn); i0=ifd(off); out={}
    subs=val(i0[330]); subs=subs if isinstance(subs,list) else [subs]
    raw=None
    for s in subs:
        sd=ifd(s)
        if val(sd[262])[0]==32803:  # CFA
            raw=sd
    out['width']=val(raw[256])[0]; out['height']=val(raw[257])[0]
    out['bps']=val(raw[258])[0]; out['compression']=val(raw[259])[0]
    out['strip_off']=val(raw[273])[0]; out['strip_len']=val(raw[279])[0]
    out['black']=val(raw[50714])[0] if 50714 in raw else 0
    out['white']=val(raw[50717])[0] if 50717 in raw else (1<<out['bps'])-1
    out['active']=val(raw[50829]) if 50829 in raw else [0,0,out['height'],out['width']]
    cfd=val(raw[33421]) if 33421 in raw else [2,2]
    cfa=val(raw[33422]) if 33422 in raw else None
    out['cfa_pattern']=cfa  # bytes: 0=R,1=G,2=B
    return out

def decode(fn, cache=True):
    nfo=info(fn)
    base=os.path.splitext(os.path.basename(fn))[0]
    os.makedirs("/tmp/raws",exist_ok=True)
    rawpath=f"/tmp/raws/{base}.raw"
    W,Hh=nfo['width'],nfo['height']
    if not (cache and os.path.exists(rawpath) and os.path.getsize(rawpath)==W*Hh*2):
        jpath=f"/tmp/{base}.ljpg"
        with open(fn,'rb') as f:
            f.seek(nfo['strip_off']); data=f.read(nfo['strip_len'])
        open(jpath,'wb').write(data)
        r=subprocess.run(["./ljpeg",jpath,rawpath,str(W),str(Hh)],capture_output=True,text=True,
                         cwd="/sessions/cool-practical-dijkstra/mnt/outputs")
        if r.returncode!=0: raise RuntimeError(r.stderr)
        os.remove(jpath)
    arr=np.fromfile(rawpath,dtype='<u2').reshape(Hh,W).astype(np.float32)
    return arr, nfo

if __name__=='__main__':
    import sys
    a,n=decode(sys.argv[1])
    print(n)
    print("shape",a.shape,"min",a.min(),"max",a.max(),"mean",a.mean())
