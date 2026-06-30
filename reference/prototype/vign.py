import numpy as np, dng, analyze, os
B="/sessions/cool-practical-dijkstra/mnt/2026"
# frames chosen for large smooth sky, per aperture
F={'f/4':f"{B}/2026-06-20/_IGP1660.DNG",'f/5.6':f"{B}/2026-06-20/_IGP1668.DNG",
   'f/8':f"{B}/2026-06-20/_IGP1658.DNG",'f/11':f"{B}/2026-06-20/_IGP1655.DNG",
   'f/16':f"{B}/2026-06-20/_IGP1657.DNG"}
def lum_zone(g,y0,y1,x0,x1): return float(np.median(g[y0:y1,x0:x1]))
for ap,p in F.items():
    a,n=dng.decode(p); g=analyze.gplane(a,n)-n['black']
    H,W=g.shape; ph=int(H*0.10); pw=int(W*0.10); ins=int(W*0.04); insy=int(H*0.04)
    cy,cx=H//2,W//2
    cen=lum_zone(g,cy-ph//2,cy+ph//2,cx-pw//2,cx+pw//2)
    # top-left/top-right corners (sky); compare to top-centre at SAME height to cancel sky vertical gradient
    topc=lum_zone(g,insy,insy+ph,cx-pw//2,cx+pw//2)
    tl=lum_zone(g,insy,insy+ph,ins,ins+pw)
    tr=lum_zone(g,insy,insy+ph,W-ins-pw,W-ins)
    # falloff of top corners vs top-centre (controls vertical sky gradient)
    s_tl=np.log2(tl/topc); s_tr=np.log2(tr/topc)
    # full corner vs centre (raw)
    print(f"{ap:5s} {os.path.basename(p)}  topC={topc:6.0f} cen={cen:6.0f}  TLvsTopC={s_tl:+.2f}st  TRvsTopC={s_tr:+.2f}st")
