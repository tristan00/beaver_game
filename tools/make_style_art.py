"""Generates docs/style_icons.png and docs/style_mockup.png from the STYLE.md palette."""
from PIL import Image, ImageDraw, ImageFont, ImageFilter
import random, math, os
OUT = os.path.join(os.path.dirname(__file__), '..', 'docs')
S = 4
INK='#4D3321'; DIM='#94755C'; PANEL='#FCF2DE'; ROWB='#F7E8CC'; TRACK='#E0CCA8'; CHIP='#EDD6A8'
LEAF='#5CA361'; BTN='#9ECC8C'; WARN='#E69933'; BAD='#D65757'
LOG='#B9783F'; LOGD='#8F5A2E'; CUT='#F2D9A8'; BARK='#7A4A2B'; GREEN='#7CC45E'; GREEND='#4E9A52'
MUD='#A07F60'; STONE='#B9AFA8'; STONED='#8C8290'; WATER='#6FD3E0'; WATERD='#1F8AB8'; BERRY='#E0556B'

def font(sz, bold=False):
    for f in (['C:/Windows/Fonts/segoeuib.ttf'] if bold else ['C:/Windows/Fonts/segoeui.ttf']):
        try: return ImageFont.truetype(f, sz)
        except Exception: pass
    return ImageFont.load_default()

def rr(d, box, r, fill=None, outline=None, w=1): d.rounded_rectangle(box, r, fill=fill, outline=outline, width=w)
def circ(d, c, r, fill): d.ellipse((c[0]-r, c[1]-r, c[0]+r, c[1]+r), fill=fill)
def line(d, pts, w, fill):
    w=int(w); d.line(pts, fill=fill, width=w, joint='curve')
    for p in (pts[0], pts[-1]): circ(d, p, w/2, fill)

# every icon is drawn in a 100x100 box; (d) is the draw context already scaled by k
def icon(name, size=96):
    k = size*S/100
    im = Image.new('RGBA', (size*S, size*S), (0,0,0,0)); d = ImageDraw.Draw(im)
    P = lambda *a: [v*k for v in a]
    def R(x0,y0,x1,y1,r,f): rr(d, P(x0,y0,x1,y1), r*k, fill=f)
    def C(x,y,r,f): circ(d, (x*k,y*k), r*k, f)
    def L(pts,w,f): line(d, [(pts[i]*k, pts[i+1]*k) for i in range(0,len(pts),2)], w*k, f)
    # soft tile
    tile = {'res':'#F7E8CC','need':'#FBE3D6','job':'#E3F0D2','act':'#DCEFE8','map':'#D7EEF2'}[KIND[name]]
    R(2,2,98,98,28,tile)
    if name=='logs':
        for (x,y) in [(18,52),(18,70)]: R(x,y-9,x+58,y+9,9,LOG)
        R(30,22,88,40,9,LOG); R(30,43,88,61,9,LOGD) if False else None
        for (x,y) in [(30,31),(18,61),(18,79)]: pass
        C(78,31,9,CUT); C(28,61,9,CUT); C(28,79,9,CUT); C(78,31,4,'#C99A5E'); C(28,61,4,'#C99A5E'); C(28,79,4,'#C99A5E')
        R(40,52,82,70,9,LOGD); C(76,61,9,CUT); C(76,61,4,'#C99A5E')
    elif name=='branches':
        L([50,84,50,50,26,24],9,LOG); L([50,50,76,30],9,LOG); L([50,68,32,56],7,LOGD); C(26,24,5,GREEN); C(76,30,5,GREEN)
    elif name=='bark':
        R(22,20,78,82,22,BARK); R(30,26,44,76,7,'#9A6238'); R(50,30,60,70,5,'#9A6238'); R(66,34,72,64,3,'#9A6238')
    elif name=='plants':
        L([50,84,50,48],8,GREEND); d.ellipse(P(18,26,50,56),fill=GREEN); d.ellipse(P(50,18,84,52),fill='#9ADB78'); C(50,86,0,GREEN)
    elif name=='mud':
        d.ellipse(P(16,38,84,86),fill=MUD); d.ellipse(P(30,22,70,56),fill=MUD); C(38,62,5,'#B79778'); C(60,50,3,'#B79778'); C(66,72,4,'#7E6249')
    elif name=='stone':
        d.polygon(P(16,70,28,38,52,24,76,36,86,70,70,82,30,82), fill=STONE); d.polygon(P(52,24,76,36,86,70,70,82,56,56), fill=STONED)
        C(36,52,5,'#D8CFC8')
    elif name=='food':
        C(40,58,20,BERRY); C(62,58,20,BERRY); C(40,52,5,'#F59AA6'); L([50,38,50,22],6,GREEND); d.ellipse(P(52,14,76,30),fill=GREEN)
    elif name=='shelter':
        d.pieslice(P(12,28,88,104),180,360,fill=LOG); d.pieslice(P(12,28,88,104),180,360,fill=LOG)
        for y in (44,56,68): L([24+ (68-y)*0.0,y,76,y],4,LOGD)
        R(42,58,58,80,8,'#4D3321'); R(10,78,90,86,4,GREEND)
    elif name=='rest':
        C(52,50,30,'#F2C94C'); C(66,42,26,'#FBE3D6'); C(74,24,4,'#F2C94C'); C(24,28,3,'#F2C94C')
    elif name=='mood':
        C(50,50,34,'#F6C94A'); C(38,42,5,INK); C(62,42,5,INK); d.arc(P(30,40,70,74),20,160,fill=INK,width=int(5*k)); C(30,58,5,'#F59AA6'); C(70,58,5,'#F59AA6')
    elif name=='collector':
        d.pieslice(P(26,8,74,60),180,360,fill=None,outline=LOGD,width=int(5*k)); d.polygon(P(20,40,80,40,70,82,30,82),fill=LOG)
        for x in (36,50,64): L([x,46,x-2,76],3,LOGD)
        C(42,38,6,BERRY); C(56,36,6,GREEN)
    elif name=='builder':
        L([30,82,58,46],9,LOG); R(40,16,82,38,8,STONED); R(66,12,84,42,8,STONED)
    elif name=='hauler':
        R(14,36,70,66,8,LOG); R(18,28,66,40,6,LOGD); C(30,74,12,STONED); C(30,74,5,CUT); C(58,74,12,STONED); C(58,74,5,CUT); L([70,52,88,40],6,LOGD)
    elif name=='specialist':
        pts=[]
        for i in range(10):
            a=-math.pi/2+i*math.pi/5; r=34 if i%2==0 else 16; pts+= [50+r*math.cos(a), 54+r*math.sin(a)]
        d.polygon(P(*pts),fill='#F2C94C'); C(50,54,6,'#FBE3D6')
    elif name=='assign':
        C(40,32,13,LOG); d.pieslice(P(14,52,66,110),180,360,fill=LOG); R(62,40,90,52,6,LEAF); R(70,32,82,60,6,LEAF)
    elif name=='build':
        R(14,60,46,84,6,LOG); R(54,60,86,84,6,LOG); R(34,34,66,58,6,'#C99A5E'); L([50,20,50,8],6,LEAF)
    elif name=='fill':
        d.rounded_rectangle(P(18,40,82,86),14*k,outline=DIM,width=int(5*k)); L([50,10,50,48],8,LEAF); d.polygon(P(36,42,64,42,50,60),fill=LEAF)
        C(50,74,8,BTN)
    elif name=='move':
        for a in (0,90,180,270):
            ca,sa=math.cos(math.radians(a)),math.sin(math.radians(a))
            d.polygon(P(50+34*ca,50+34*sa, 50+18*ca-8*sa,50+18*sa+8*ca, 50+18*ca+8*sa,50+18*sa-8*ca),fill=LEAF)
        d.ellipse(P(32,32,68,68),outline=GREEND,width=int(5*k)); C(50,50,6,GREEND)
    elif name=='accept':
        C(50,50,34,LEAF); L([32,52,45,65,70,36],9,'#FFFAEE')
    elif name=='turn':
        C(50,50,34,'#E58B8B'); L([36,36,64,64],9,'#FFFAEE'); L([64,36,36,64],9,'#FFFAEE')
    elif name=='water':
        d.polygon(P(50,12,76,56,50,60,24,56),fill=WATER); C(50,60,27,WATER); C(40,56,6,'#E8FAFA'); L([28,82,40,78,50,82,62,78,72,82],4,WATERD)
    elif name=='forest':
        L([50,88,50,62],10,LOGD); C(50,40,24,GREEN); C(34,54,16,GREEN); C(66,54,16,GREEN); C(44,34,7,'#9ADB78')
    elif name=='foodmap':
        C(34,62,17,BERRY); C(66,62,17,BERRY); C(50,38,17,BERRY); C(50,32,4,'#F59AA6'); L([50,22,50,10],5,GREEND)
    elif name=='work':
        for i in range(8):
            a=i*math.pi/4; C(50+30*math.cos(a),50+30*math.sin(a),9,STONED)
        C(50,50,28,STONED); C(50,50,12,tile)
    return im.resize((size,size), Image.LANCZOS)

KIND={}
SETS=[('Resources','res',[('logs','Logs'),('branches','Branches'),('bark','Bark'),('plants','Plants'),('mud','Mud'),('stone','Stone')]),
 ('Needs','need',[('food','Food'),('shelter','Shelter'),('rest','Rest'),('mood','Mood')]),
 ('Jobs','job',[('collector','Collector'),('builder','Builder'),('hauler','Hauler'),('specialist','Specialist')]),
 ('Actions','act',[('assign','Assign'),('build','Build'),('fill','Fill slot'),('move','Move work area'),('accept','Accept'),('turn','Turn away')]),
 ('Map modes','map',[('water','Water'),('forest','Forest'),('foodmap','Food'),('work','Work')])]
for _,k_,items in SETS:
    for n,_l in items: KIND[n]=k_

def sheet():
    W,H=900,760; im=Image.new('RGB',(W,H),PANEL); d=ImageDraw.Draw(im); y=16
    for title,_,items in SETS:
        d.text((20,y),title,font=font(16,True),fill=INK); y+=26
        for i,(n,l) in enumerate(items):
            x=20+i*140; ico=icon(n,80); im.paste(ico,(x,y),ico); d.text((x,y+84),l,font=font(13),fill=DIM)
        y+=116
    im.save(os.path.join(OUT,'style_icons.png'))

# ---------- mockup ----------
def mockup():
    W,H=1280,720; random.seed(7)
    im=Image.new('RGB',(W,H),'#9ED1F5'); d=ImageDraw.Draw(im,'RGBA')
    for y in range(H):
        t=y/H; d.line([(0,y),(W,y)],fill=(int(168-40*t),int(216-24*t),int(120-18*t)))
    d.rectangle((0,0,W,150),fill='#9ED1F5')
    for cx,cy,r in [(180,230,140),(600,200,200),(1050,230,160)]: d.ellipse((cx-r,cy-r*0.55,cx+r,cy+r*0.9),fill='#B2DA88')
    d.rectangle((0,260,W,H),fill='#92CC6C')
    for _ in range(40):
        x,y,r=random.randint(0,W),random.randint(270,H),random.randint(60,140); d.ellipse((x-r,y-r//3,x+r,y+r//3),fill=random.choice(['#A3D77A','#85C468','#9ED173']))
    d.polygon([(0,200),(150,170),(300,230),(380,300),(0,320)],fill='#B9AFA8'); d.polygon([(0,260),(250,250),(380,300),(0,320)],fill='#8C8290')
    pond=Image.new('RGBA',(W,H),(0,0,0,0)); p=ImageDraw.Draw(pond)
    p.ellipse((380,350,1080,690),fill='#1F8AB8'); p.ellipse((410,372,1050,660),fill='#6FD3E0'); p.ellipse((520,410,900,600),fill='#8FE0E6')
    pond=pond.filter(ImageFilter.GaussianBlur(1.2)); im.paste(pond,(0,0),pond); d=ImageDraw.Draw(im,'RGBA')
    d.ellipse((380,350,1080,690),outline='#F4FBF6',width=5)
    for _ in range(26):
        x,y=random.randint(470,980),random.randint(400,640); d.arc((x,y,x+30,y+9),200,340,fill='#F4FBF6',width=2)
    for x,y in [(600,500),(640,520),(820,460),(860,480)]: d.ellipse((x,y,x+26,y+11),fill='#4E9A52')
    for _ in range(150):
        x,y=random.randint(20,W-20),random.randint(300,H-100)
        if 380<x<1080 and 350<y<690: continue
        k=random.random()
        if k<.4: d.line([(x,y),(x-3,y-8)],fill='#4E9A52',width=2); d.line([(x,y),(x+3,y-8)],fill='#4E9A52',width=2)
        elif k<.7: d.ellipse((x,y,x+5,y+5),fill=random.choice(['#FFFAEE','#F59AA6','#F2C94C']))
        else: d.ellipse((x,y,x+10,y+6),fill='#B9AFA8')
    def tree(x,y,s=1):
        d.ellipse((x-24*s,y+30*s,x+24*s,y+44*s),fill=(77,51,100,60))
        d.rectangle((x-5*s,y+8*s,x+5*s,y+38*s),fill='#8F5A2E')
        for dx,dy,r,c in [(0,-14,28,'#5FA85A'),(-16,6,22,'#6DB863'),(16,6,22,'#6DB863'),(-6,-22,12,'#8CD070')]: d.ellipse((x+dx*s-r*s,y+dy*s-r*s,x+dx*s+r*s,y+dy*s+r*s),fill=c)
    for x,y in [(110,330),(200,420),(70,520),(170,610),(300,540),(1150,330),(1220,430),(1120,520),(1190,610),(1060,250),(420,300),(980,300)]: tree(x,y,1.0 if y>400 else .8)
    d.ellipse((620,350,760,392),fill=(77,51,100,60)); d.pieslice((620,230,760,370),180,360,fill='#B9783F'); d.rectangle((620,300,760,372),fill='#B9783F'); d.ellipse((620,350,760,392),fill='#B9783F')
    for yy in (270,296,322): d.line([(640+(yy-270)*0.3,yy),(740-(yy-270)*0.3,yy)],fill='#8F5A2E',width=4)
    d.rounded_rectangle((676,330,704,372),12,fill='#4D3321')
    for i in range(10): d.rounded_rectangle((720+i*26,520-i*8,752+i*26,538-i*8),9,fill='#B9783F',outline='#8F5A2E',width=2)
    def beaver(x,y,flip=1,carry=None):
        d.ellipse((x-16,y+14,x+20,y+22),fill=(77,51,100,60)); d.ellipse((x-14,y-8,x+14,y+16),fill='#B9783F'); d.ellipse((x+8*flip-8,y-18,x+8*flip+10,y),fill='#C98B52')
        d.ellipse((x+8*flip-5,y-16,x+8*flip-1,y-12),fill=INK); d.ellipse((x-24*flip-9,y+6,x-24*flip+9,y+14),fill='#8F5A2E')
        if carry: d.rounded_rectangle((x-12,y-14,x+12,y-6),4,fill=carry)
    for x,y,f,c in [(560,300,1,None),(500,320,-1,'#8F5A2E'),(820,300,1,None),(860,540,-1,None),(430,470,1,None),(600,610,1,None)]: beaver(x,y,f,c)
    ring=Image.new('RGBA',(W,H),(0,0,0,0)); r=ImageDraw.Draw(ring); r.ellipse((450,240,720,380),fill=(158,204,140,60),outline=(92,163,97,230),width=4)
    im.paste(ring,(0,0),ring)
    def shadow(box,rad):
        sh=Image.new('RGBA',(W,H),(0,0,0,0)); s=ImageDraw.Draw(sh); s.rounded_rectangle((box[0],box[1]+4,box[2],box[3]+4),rad,fill=(77,51,100,60)); sh=sh.filter(ImageFilter.GaussianBlur(5)); im.paste(sh,(0,0),sh)
    def pill(box,text,fill,f=13):
        dd=ImageDraw.Draw(im); dd.rounded_rectangle(box,(box[3]-box[1])//2,fill=fill); ff=font(f,True); tw=dd.textlength(text,font=ff); dd.text(((box[0]+box[2])/2-tw/2,(box[1]+box[3])/2-f*0.68),text,font=ff,fill=INK)
    def ic(n,x,y,s=28): i=icon(n,s); im.paste(i,(int(x),int(y)),i)
    def card(box,rad=22):
        shadow(box,rad); ImageDraw.Draw(im).rounded_rectangle(box,rad,fill=PANEL)
    card((200,10,1080,54))
    d=ImageDraw.Draw(im); x=216
    for n,v in [('logs','46'),('branches','41'),('bark','12'),('plants','28'),('mud','19'),('stone','8')]:
        ic(n,x,16,26); d.text((x+30,21),v,font=font(14,True),fill=INK); x+=68
    d.line([(x,20),(x,44)],fill=TRACK,width=2); x+=14
    for n,v in [('food','Food 24d'),('shelter','Homes 9/9'),('rest','Rest 80%'),('mood','Mood 75%')]:
        ic(n,x,16,26); d.text((x+30,21),v,font=font(13,True),fill=INK); x+=int(d.textlength(v,font=font(13,True)))+38
    pill((16,14,176,50),'Spring  Year 2 · Day 14',CHIP)
    card((1100,10,1264,54))
    for i,t in enumerate(['II','1x','3x']): pill((1110+i*50,16,1152+i*50,48),t,BTN if i==1 else CHIP)
    card((16,70,246,250)); d=ImageDraw.Draw(im); d.text((32,82),'JOBS',font=font(11,True),fill=DIM)
    for i,(n,l,v,fr) in enumerate([('collector','Collector',9,.6),('builder','Builder',3,.25),('hauler','Hauler',4,.35),('specialist','Specialist',2,.15)]):
        y=102+i*36; ic(n,30,y,28); d.text((66,y+4),l,font=font(13),fill=INK); d.rounded_rectangle((140,y+10,206,y+20),5,fill=TRACK); d.rounded_rectangle((140,y+10,140+66*fr,y+20),5,fill=LEAF); d.text((214,y+4),str(v),font=font(13,True),fill=INK)
    card((1000,70,1264,250)); d=ImageDraw.Draw(im)
    ic('assign',1014,82,30); d.text((1052,86),'Wanderers ask to join',font=font(14,True),fill=INK)
    d.text((1016,124),'Two beavers wait at the edge of\nthe meadow, hopeful.',font=font(12),fill=DIM)
    d.rounded_rectangle((1014,170,1138,230),16,fill=BTN); ic('accept',1022,184,32); d.text((1060,190),'Accept',font=font(13,True),fill=INK)
    d.rounded_rectangle((1146,170,1250,230),16,fill=ROWB); ic('turn',1152,184,32); d.text((1188,184),'Turn\naway',font=font(12,True),fill=INK)
    card((730,150,950,226),16); d=ImageDraw.Draw(im); d.polygon([(780,226),(800,226),(790,238)],fill=PANEL)
    ic('shelter',740,158,26); d.text((772,162),'Lodge',font=font(14,True),fill=INK); d.text((742,194),'Beds 2/4   Fill slot: click +',font=font(12),fill=DIM); ic('fill',918,158,24)
    card((340,640,940,704),30); d=ImageDraw.Draw(im)
    for i,(n,l) in enumerate([('water','Water'),('forest','Forest'),('foodmap','Food'),('work','Work')]):
        x0=356+i*92; d.rounded_rectangle((x0,648,x0+84,696),22,fill=BTN if n=='work' else CHIP); ic(n,x0+6,656,32); d.text((x0+42,664),l,font=font(12,True),fill=INK)
    d.line([(732,654),(732,690)],fill=TRACK,width=2)
    for i,(n,l) in enumerate([('build','Build'),('assign','Assign'),('move','Move area')]):
        x0=744+i*64; d.rounded_rectangle((x0,648,x0+58,696),22,fill=ROWB); ic(n,x0+15,652,28); d.text((x0+29-d.textlength(l,font=font(9,True))/2,682),l,font=font(9,True),fill=INK)
    glow=Image.new('RGBA',(W,H),(0,0,0,0)); g=ImageDraw.Draw(glow); g.ellipse((-300,-300,400,300),fill=(255,245,224,50)); glow=glow.filter(ImageFilter.GaussianBlur(80)); im.paste(glow,(0,0),glow)
    im.convert('RGB').save(os.path.join(OUT,'style_mockup.png'))

if __name__=='__main__': sheet(); mockup()
