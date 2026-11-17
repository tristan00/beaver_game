"""Draws assets/icons/*.png (96 px) and docs/icons_sheet.png (32 + 64 px) per docs/STYLE.md section 7.
Run: python tools/make_icons.py
Files are named by game::ui::icons::Icon::file() (lowercase variant name); the style-guide names
that have no enum variant (plants, mood, collector, builder, hauler, specialist, fillslot) are kept too."""
from PIL import Image, ImageDraw, ImageFont
import math, os
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
OUT = os.path.join(ROOT, 'assets', 'icons'); DOCS = os.path.join(ROOT, 'docs')
S = 4
INK='#4D3321'; DIM='#94755C'; PANEL='#FCF2DE'; CREAM='#FFFAEE'
LEAF='#5CA361'; BTN='#9ECC8C'; BERRY='#E0556B'; PINK='#F59AA6'; SUN='#F2C94C'
LOG='#B9783F'; LOGD='#8F5A2E'; CUT='#F2D9A8'; CUTD='#C99A5E'; BARK='#7A4A2B'; BARKL='#9A6238'
GREEN='#7CC45E'; GREEND='#4E9A52'; GREENL='#B4E08F'; MUD='#A07F60'; MUDL='#B79778'; MUDD='#7E6249'
STONE='#B9AFA8'; STONED='#8C8290'; STONEL='#D8CFC8'; WATER='#6FD3E0'; WATERD='#1F8AB8'; ROSE='#E58B8B'
TILES={'res':'#F7E8CC','need':'#FBE3D6','job':'#E3F0D2','act':'#DCEFE8','map':'#D7EEF2','nav':'#EDE3F5'}

# drawing primitives, rebound for each icon by draw(); coordinates are a 100x100 box
d = k = None
def P(*a): return [v*k for v in a]
def R(x0,y0,x1,y1,r,f): d.rounded_rectangle(P(x0,y0,x1,y1),r*k,fill=f)
def C(x,y,r,f): d.ellipse(P(x-r,y-r,x+r,y+r),fill=f)
def E(x0,y0,x1,y1,f): d.ellipse(P(x0,y0,x1,y1),fill=f)
def L(pts,w,f):
    q=[(pts[i]*k,pts[i+1]*k) for i in range(0,len(pts),2)]
    d.line(q,fill=f,width=int(w*k),joint='curve')
    for p in (q[0],q[-1]): d.ellipse((p[0]-w*k/2,p[1]-w*k/2,p[0]+w*k/2,p[1]+w*k/2),fill=f)
def HL(x,y,r=3.2): C(x,y,r,CREAM)   # the small highlight dot
def RING(cx,cy,r,w,f): d.ellipse(P(cx-r,cy-r,cx+r,cy+r),outline=f,width=int(w*k))
def ARC(box,a0,a1,w,f): d.arc(P(*box),a0,a1,fill=f,width=int(w*k))

def draw(fn, kind, size):
    global d, k
    k=size*S/100
    im=Image.new('RGBA',(size*S,size*S),(0,0,0,0)); d=ImageDraw.Draw(im)
    R(2,2,98,98,28,TILES[kind])
    fn()
    return im.resize((size,size),Image.LANCZOS)

def logend(x,y,r=9): C(x,y,r,CUT); C(x,y,r*0.55,CUTD); C(x,y,r*0.22,CUT)

def logs():
    R(16,56,80,76,10,LOGD); logend(76,66,10)
    R(16,36,80,56,10,LOG);  logend(76,46,10)
    R(30,16,84,36,10,LOG);  logend(80,26,10); HL(28,26)
def branches():
    L([48,86,48,52,28,30],9,LOG); L([48,52,74,32],9,LOG); L([48,70,32,60],7,LOGD)
    E(14,14,38,34,GREEN); E(66,16,90,38,GREEN); C(30,22,3,GREENL); HL(40,70,2.6)
def bark():
    R(22,16,78,84,24,BARK); R(31,24,43,76,6,BARKL); R(50,28,60,72,5,BARKL); R(67,34,73,66,3,BARKL); HL(64,22)
def plants():
    L([50,86,50,50],8,GREEND); E(16,24,52,58,GREEN); E(48,16,86,52,'#9ADB78'); HL(30,36)
def mud():
    E(14,40,86,86,MUD); E(28,22,72,60,MUD); C(38,62,6,MUDL); C(62,50,4,MUDL); C(68,72,5,MUDD); C(26,74,3.5,MUDD); HL(44,38)
def stone():
    R(14,34,86,84,24,STONE); R(40,34,86,84,22,STONED); R(14,34,66,84,24,STONE)
    E(22,40,46,60,STONEL); C(60,66,5,STONED); HL(34,50,3)
def food():
    C(36,62,20,BERRY); C(64,62,20,BERRY); L([50,44,50,26],6,GREEND); E(52,12,80,32,GREEN)
    C(30,55,5,PINK); C(58,55,5,PINK)
def shelter():
    d.pieslice(P(12,22,88,98),180,360,fill=LOG)
    for y in (44,54): L([24,y,76,y],4,LOGD)
    R(40,56,60,82,10,INK); d.rectangle(P(40,70,60,82),fill=INK); R(8,78,92,88,5,GREEND); HL(34,36)
def rest():
    C(50,50,32,SUN); C(66,42,27,TILES['need']); C(78,22,4,SUN); C(24,26,3.2,SUN); C(30,76,2.6,SUN)
def mood():
    C(50,50,35,SUN); C(37,42,5,INK); C(63,42,5,INK); ARC((31,38,69,74),25,155,5,INK)
    C(29,58,6,PINK); C(71,58,6,PINK); HL(34,28)
def collector():
    ARC((26,8,74,64),180,360,6,LOGD); R(18,38,82,84,18,LOG)
    for x in (34,50,66): L([x,46,x-1,76],3,LOGD)
    C(40,34,8,BERRY); E(50,22,70,38,GREEN); HL(36,31,2.4)
def builder():
    L([28,84,58,44],10,LOG); R(40,14,84,40,10,STONED); R(40,14,62,40,10,STONE); HL(50,24)
def hauler():
    R(12,34,72,66,12,LOG); R(18,24,68,40,8,LOGD); logend(66,32,6)
    C(28,74,13,STONED); C(28,74,5,CUT); C(58,74,13,STONED); C(58,74,5,CUT); L([72,52,90,38],7,LOGD)
def specialist():
    pts=[]
    for i in range(10):
        a=-math.pi/2+i*math.pi/5; r=30 if i%2==0 else 17; pts+=[50+r*math.cos(a),54+r*math.sin(a)]
    d.polygon(P(*pts),fill=SUN)
    for i in range(5):
        a=-math.pi/2+i*2*math.pi/5; C(50+30*math.cos(a),54+30*math.sin(a),6,SUN)
    C(50,54,19,SUN); HL(43,46,4)
def beaver(x,y,s=1.0):
    E(x-26*s,y+14*s,x+26*s,y+66*s,LOG); C(x,y,17*s,LOG)
    C(x-6*s,y-3*s,2.5*s,INK); C(x+6*s,y-3*s,2.5*s,INK); R(x-5*s,y+4*s,x+5*s,y+11*s,3*s,CUT)
def assign():
    beaver(38,34); R(62,42,92,56,7,LEAF); R(70,34,84,64,7,LEAF)
def build():
    R(10,58,46,86,8,LOG); R(54,58,90,86,8,LOG); R(32,32,68,58,8,CUTD); L([50,22,50,10],6,LEAF)
    E(50,6,72,20,GREEN); HL(20,68)
def fillslot():
    R(16,44,84,88,16,TILES['act']); d.rounded_rectangle(P(16,44,84,88),16*k,outline=DIM,width=int(6*k))
    L([50,8,50,46],9,LEAF); d.polygon(P(33,40,67,40,50,60),fill=LEAF); C(50,74,7,BTN)
def movearea():
    for a in (0,90,180,270):
        ca,sa=math.cos(math.radians(a)),math.sin(math.radians(a))
        C(50+30*ca,50+30*sa,10,LEAF)
        d.polygon(P(50+38*ca,50+38*sa,50+24*ca-10*sa,50+24*sa+10*ca,50+24*ca+10*sa,50+24*sa-10*ca),fill=LEAF)
    RING(50,50,16,5,GREEND); C(50,50,5,GREEND)
def accept():
    C(50,50,36,LEAF); L([32,52,45,65,69,36],10,CREAM); HL(36,34)
def turnaway():
    C(50,50,36,ROSE); L([36,36,64,64],10,CREAM); L([64,36,36,64],10,CREAM)
def modewater():
    C(50,52,25,WATER); d.polygon(P(50,10,73,48,27,48),fill=WATER); C(40,50,6,'#E8FAFA'); L([22,82,36,76,50,82,64,76,78,82],5,WATERD)
def modeforest():
    L([50,90,50,62],11,LOGD); C(50,38,24,GREEN); C(33,54,17,GREEN); C(67,54,17,GREEN); C(43,32,7,GREENL)
def modefood():
    C(32,64,18,BERRY); C(68,64,18,BERRY); C(50,38,18,BERRY); L([50,22,50,10],5,GREEND); E(52,8,72,22,GREEN); C(44,32,4,PINK)
def modework():
    for i in range(8):
        a=i*math.pi/4; C(50+31*math.cos(a),50+31*math.sin(a),9.5,STONED)
    C(50,50,29,STONED); C(50,50,13,TILES['map']); C(40,38,5,STONEL)
# ---- extras named by the Icon enum, same style ----
def baskets():
    ARC((24,10,76,70),180,360,6,LOGD); R(16,40,84,84,20,CUTD)
    for y in (52,66): L([26,y,74,y],4,LOG)
    HL(30,48)
def rope():
    RING(50,52,30,8,CUTD); RING(50,52,15,8,CUTD); L([62,52,86,72],8,CUTD); HL(30,34,2.6)
def tools():
    L([26,84,60,42],9,LOG); R(44,14,82,36,9,STONED); L([74,84,40,42],8,LOGD); C(34,36,10,STONE); C(34,36,4,CREAM)
def carry():
    R(18,44,82,66,11,LOG); logend(76,55,9); L([26,80,26,70],7,LOGD); L([68,80,68,70],7,LOGD)
    L([50,34,50,16],7,LEAF); d.polygon(P(38,26,62,26,50,10),fill=LEAF); HL(30,50)
def cancel():
    RING(50,50,32,7,DIM); L([36,36,64,64],8,DIM); L([64,36,36,64],8,DIM)
def demolish():
    R(12,60,48,88,8,LOG); R(54,68,90,88,8,LOG); L([50,18,38,48],9,STONED); R(46,10,86,32,10,STONED); R(46,10,66,32,10,STONE)
def navbeavers(): beaver(50,28,1.15)
def navstores():
    R(14,34,86,86,16,LOG); R(14,34,86,52,10,LOGD); R(40,58,60,70,5,CUT); HL(24,44)
def navtech():
    C(50,40,26,SUN); R(38,62,62,78,8,STONED); R(42,76,58,88,6,STONE); L([44,44,50,52,56,44],4,'#C99A2E'); HL(40,30,4)
def navevents():
    R(16,18,84,84,18,CREAM); R(16,18,84,38,14,LEAF); L([34,52,66,52],6,CUTD); L([34,68,56,68],6,CUTD); C(70,70,6,BERRY)

ICONS=[ # (file, fn, tile kind, label, in the style guide list)
 ('logs',logs,'res','Logs',1),('branches',branches,'res','Branches',1),('bark',bark,'res','Bark',1),
 ('plants',plants,'res','Plants',1),('mud',mud,'res','Mud',1),('stone',stone,'res','Stone',1),
 ('food',food,'need','Food',1),('shelter',shelter,'need','Shelter',1),('rest',rest,'need','Rest',1),('mood',mood,'need','Mood',1),
 ('collector',collector,'job','Collector',1),('builder',builder,'job','Builder',1),('hauler',hauler,'job','Hauler',1),('specialist',specialist,'job','Specialist',1),
 ('assign',assign,'act','Assign',1),('build',build,'act','Build',1),('fillslot',fillslot,'act','Fill slot',1),('movearea',movearea,'act','Move area',1),
 ('accept',accept,'act','Accept',1),('turnaway',turnaway,'act','Turn away',1),
 ('modewater',modewater,'map','Water',1),('modeforest',modeforest,'map','Forest',1),('modefood',modefood,'map','Food',1),('modework',modework,'map','Work',1),
 ('baskets',baskets,'res','Baskets',0),('rope',rope,'res','Rope',0),('tools',tools,'res','Tools',0),
 ('social',mood,'need','Social',0),('collect',collector,'job','Collect',0),('carry',carry,'job','Carry',0),
 ('haul',hauler,'job','Haul',0),('craft',builder,'job','Craft',0),
 ('cancel',cancel,'act','Cancel',0),('demolish',demolish,'act','Demolish',0),
 ('navcolony',shelter,'nav','Colony',0),('navbeavers',navbeavers,'nav','Beavers',0),('navstores',navstores,'nav','Stores',0),
 ('navbuild',build,'nav','Build',0),('navtech',navtech,'nav','Tech',0),('navevents',navevents,'nav','Events',0)]

def font(sz,b=False):
    try: return ImageFont.truetype('C:/Windows/Fonts/segoeui%s.ttf'%('b' if b else ''),sz)
    except Exception: return ImageFont.load_default()

def main():
    os.makedirs(OUT,exist_ok=True)
    for f,fn,kind,_l,_g in ICONS: draw(fn,kind,96).save(os.path.join(OUT,f+'.png'))
    guide=[i for i in ICONS if i[4]]
    groups=[('Resources','res'),('Needs','need'),('Jobs','job'),('Actions','act'),('Map modes','map')]
    W=700; im=Image.new('RGB',(W,700),PANEL); dd=ImageDraw.Draw(im); y=14
    dd.text((20,y),'Icon set: each shown at 64 px (left) and 32 px (right)',font=font(15,True),fill=INK); y+=34
    for title,kind in groups:
        dd.text((20,y),title,font=font(14,True),fill=INK); y+=22
        for n,(f,fn,_k,lab,_g) in enumerate([i for i in guide if i[2]==kind]):
            x=20+n*108
            i64=draw(fn,kind,64); im.paste(i64,(x,y),i64)
            i32=draw(fn,kind,32); im.paste(i32,(x+68,y+16),i32)
            dd.text((x,y+68),lab,font=font(12),fill=DIM)
        y+=100
    im.crop((0,0,W,y+4)).save(os.path.join(DOCS,'icons_sheet.png'))
main()
