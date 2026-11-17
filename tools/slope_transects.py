import numpy as np, json
from PIL import Image, ImageDraw
A="artifacts/R19-29/"
W=np.array([.2126,.7152,.0722])
out={}
for n in ["iso_seed1","iso_seed2","lodge_seed1","lodge_seed2","pond_seed1","pond_seed2"]:
    im=np.asarray(Image.open(A+f"after/{n}_noglow.png").convert("RGB")).astype(float)
    m=np.asarray(Image.open(A+f"masks/{n}_mask.png").convert("RGB")).astype(int)
    lit=np.all(np.abs(m-np.array([255,255,0]))<6,axis=2)
    lum=im@W
    runs=[]
    for y in range(0,lit.shape[0],3):
        row=lit[y]; x=0
        while x<len(row):
            if row[x]:
                x0=x
                while x<len(row) and row[x]: x+=1
                if x-x0>=100:
                    seg=lum[y,x0+1:x-1]; d=np.abs(np.diff(seg)); rng=seg.max()-seg.min()
                    if rng>=25:
                        k=int(d.argmax())+x0+1
                        near=(~lit[max(0,y-8):y+9,max(0,k-8):k+9]).any()
                        runs.append(dict(y=y,x0=x0+1,x1=x-2,rng=float(rng),jump=float(d.max()),explained=bool(near)))
            else: x+=1
    ok=[r for r in runs if r["jump"]<=8]; bad=[r for r in runs if r["jump"]>8]
    unexpl=[r for r in bad if not r["explained"]]
    print(n,"runs",len(runs),"pass(<=8)",len(ok),"fail",len(bad),"fail not next to a prop/shadow pixel",len(unexpl))
    top=sorted(ok,key=lambda r:-r["rng"])[:2]
    print("   best passing transects (y,x0,x1,lum range,max jump):",[(r["y"],r["x0"],r["x1"],round(r["rng"]),round(r["jump"],1)) for r in top])
    out[n]=dict(top=top,unexplained=unexpl[:3])
    if top:
        img=Image.open(A+f"after/{n}.png").convert("RGB"); dr=ImageDraw.Draw(img)
        for r in top: dr.line([(r["x0"],r["y"]),(r["x1"],r["y"])],fill=(255,0,255),width=2)
        img.save(A+f"transect_{n}.png")
    for r in unexpl[:2]: print("   unexplained fail:",r)
json.dump(out,open("tmp/tr.json","w"))
