"""Measurement engine: green-plane acutance per zone, decentring, vignetting helpers."""
import numpy as np, dng, os

def gplane(a, nfo):
    """Return a single green sub-mosaic (properly sampled on its own grid) as float32.
    CFA pattern bytes give site colours for the 2x2 (0=R,1=G,2=B)."""
    pat=nfo['cfa_pattern']
    # positions in 2x2: (0,0)=pat[0],(0,1)=pat[1],(1,0)=pat[2],(1,1)=pat[3]
    greens=[(0,0),(0,1),(1,0),(1,1)]
    gi=[p for p,(r,c) in zip([pat[0],pat[1],pat[2],pat[3]],greens) if p==1]
    # pick the first green site
    sites=[(0,0),(0,1),(1,0),(1,1)]
    gsite=None
    for k,(r,c) in enumerate(sites):
        if [pat[0],pat[1],pat[2],pat[3]][k]==1: gsite=(r,c); break
    r,c=gsite
    return a[r::2, c::2]

def _k(sigma):
    n=int(sigma*3)*2+1; x=np.arange(n)-n//2
    k=np.exp(-(x**2)/(2*sigma*sigma)); return (k/k.sum()).astype(np.float32)

def blur(img,sigma):
    k=_k(sigma)
    out=np.apply_along_axis(lambda m:np.convolve(m,k,'same'),0,img)
    out=np.apply_along_axis(lambda m:np.convolve(m,k,'same'),1,out)
    return out

def acutance(patch):
    """Scene-normalised sharpness: ratio of high-freq to mid-freq RMS energy."""
    p=patch.astype(np.float32)
    b1=blur(p,1.0); b2=blur(p,2.5)
    hp=p-b1; mp=b1-b2
    shp=hp.std(); smp=mp.std()
    return float(shp/smp) if smp>1e-6 else 0.0, float(p.std()/ (p.mean()+1e-6))

def zones(g, frac=0.13, inset=0.045):
    """Return dict of (y0,y1,x0,x1) for 5 zones on green plane g."""
    H,W=g.shape; ph=int(H*frac); pw=int(W*frac); ins_y=int(H*inset); ins_x=int(W*inset)
    cy,cx=H//2,W//2
    return {
        'C' :(cy-ph//2,cy+ph//2,cx-pw//2,cx+pw//2),
        'TL':(ins_y,ins_y+ph, ins_x,ins_x+pw),
        'TR':(ins_y,ins_y+ph, W-ins_x-pw,W-ins_x),
        'BL':(H-ins_y-ph,H-ins_y, ins_x,ins_x+pw),
        'BR':(H-ins_y-ph,H-ins_y, W-ins_x-pw,W-ins_x),
    }

def measure(path):
    a,n=dng.decode(path); g=gplane(a,n)
    z=zones(g); out={}
    for name,(y0,y1,x0,x1) in z.items():
        ac,con=acutance(g[y0:y1,x0:x1])
        out[name]=dict(acut=ac, contrast=con)
    return out,g,n

if __name__=='__main__':
    import sys,glob
    for p in sys.argv[1:]:
        m,_,_=measure(p)
        s=" ".join(f"{k}={m[k]['acut']:.3f}(c{m[k]['contrast']:.2f})" for k in ['TL','TR','C','BL','BR'])
        print(os.path.basename(p), s)
