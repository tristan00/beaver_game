"""Draws assets/icons/*.png (96 px) and docs/icons_sheet.png (32 + 64 px) per docs/UI_SPEC.md section 6 (icons are interface art; docs/STYLE.md covers the world only).
Run: python tools/make_icons.py
Files are named by game::ui::icons::Icon::file() (lowercase variant name); every file here has a
variant, and the game loads all of them at startup (ui::icons::IconsPlugin)."""
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
TILES={'res':'#F7E8CC','need':'#FBE3D6','job':'#E3F0D2','act':'#DCEFE8','map':'#D7EEF2','nav':'#EDE3F5','bld':'#EADFCB','alert':'#F8D9D2','season':'#DDEBF5','speed':'#E3E8DA'}

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


# ---- buildings (one per BuildingKind), alerts, seasons, speed and the rest of the UI spec list ----
ORANGE='#E08A3C'; ICE='#5BA8D0'; ORANGED='#B05A22'; WATERL='#E8FAFA'
def waves(y,x0=10,x1=90,w=5,f=None):
    f=f or WATERD; pts=[]
    for i in range(int((x1-x0)/10)+1): pts+=[x0+i*10,y+(3 if i%2 else -3)]
    L(pts,w,f)
def roof(x0,x1,ytop,ybase,f): d.polygon(P(x0,ybase,(x0+x1)/2,ytop,x1,ybase),fill=f)
def arrow(x0,y0,x1,y1,w,f,hs=13):
    L([x0,y0,x1,y1],w,f); a=math.atan2(y1-y0,x1-x0); ca,sa=math.cos(a),math.sin(a)
    d.polygon(P(x1+hs*ca,y1+hs*sa,x1-hs*0.2*ca-hs*sa,y1-hs*0.2*sa+hs*ca,x1-hs*0.2*ca+hs*sa,y1-hs*0.2*sa-hs*ca),fill=f)
def badge(x=80,y=20):
    C(x,y,15,BERRY); L([x,y-8,x,y+1],4.2,CREAM); C(x,y+8,2.6,CREAM)
def dome(cx,cy,r):
    d.pieslice(P(cx-r,cy-r,cx+r,cy+r),180,360,fill=LOG)
    for f in (0.35,0.7): L([cx-r*0.82*(1-f*0.3),cy-r*f,cx+r*0.82*(1-f*0.3),cy-r*f],3,LOGD)
def door(x0,y0,x1,y1): R(x0,y0,x1,y1,(x1-x0)/2,INK); d.rectangle(P(x0,(y0+y1)/2,x1,y1),fill=INK)
def snow(cx,cy,r,w,f):
    for i in range(3):
        a=i*math.pi/3; L([cx-r*math.cos(a),cy-r*math.sin(a),cx+r*math.cos(a),cy+r*math.sin(a)],w,f)
    for i in range(6):
        a=i*math.pi/3; bx,by=cx+r*0.62*math.cos(a),cy+r*0.62*math.sin(a)
        for sgn in (-1,1):
            t=a+sgn*math.pi/4; L([bx,by,bx+r*0.28*math.cos(t),by+r*0.28*math.sin(t)],w*0.8,f)
def cog(cx,cy,r,f,hole):
    for i in range(8):
        a=i*math.pi/4; C(cx+r*1.05*math.cos(a),cy+r*1.05*math.sin(a),r*0.32,f)
    C(cx,cy,r,f); C(cx,cy,r*0.45,hole)

def dam():
    R(8,64,92,90,10,WATER); waves(77,w=4)
    R(10,44,90,64,10,LOGD); logend(84,54,8); R(14,26,78,46,10,LOG); logend(72,36,8); HL(24,36)
    L([20,26,16,12],5,LOGD); L([40,26,42,10],5,LOGD)
def lodge():
    dome(50,76,42); door(40,58,60,80); R(8,78,92,90,6,WATER); waves(84,w=3); L([62,36,74,14],5,LOGD); HL(32,48,3)
def burrow():
    d.pieslice(P(8,38,92,122),180,360,fill=MUD); d.pieslice(P(36,58,64,94),180,360,fill=INK)
    R(6,80,94,90,5,GREEND); C(24,64,4,MUDL); C(76,64,3.5,MUDD); HL(32,50,3)
def foodcache():
    d.pieslice(P(10,38,90,118),180,360,fill=LOG)
    for y in (66,76): L([20,y,80,y],4,LOGD)
    C(36,36,11,BERRY); C(58,34,11,BERRY); C(47,22,11,BERRY); E(56,6,80,20,GREEN); C(32,32,3,PINK); R(8,78,92,90,6,GREEND)
def scentmound():
    E(12,52,88,90,MUD); E(28,38,72,70,MUD); C(40,72,6,MUDL); C(66,76,5,MUDD)
    for x in (34,50,66): L([x,28,x+5,21,x-1,14,x+4,8],4,GREEND)
def guardpost():
    for x,top in ((10,34),(32,14),(54,14),(76,34)):
        R(x,top,x+16,88,3,LOG); d.polygon(P(x,top+2,x+8,top-12,x+16,top+2),fill=LOG)
    R(6,62,94,72,3,LOGD); HL(18,50,2.6)
def canal():
    R(6,18,94,36,8,GREEN); R(6,64,94,82,8,GREEN); d.rectangle(P(6,30,94,70),fill=WATER); waves(50,w=4); R(6,74,94,88,6,GREEND); HL(20,27,2.5)
def lock():
    R(6,14,34,86,8,GREEN); R(66,14,94,86,8,GREEN); d.rectangle(P(34,24,66,86),fill=WATER); d.rectangle(P(34,54,66,86),fill=WATERD)
    R(42,20,58,82,4,LOG); L([50,24,50,78],3,LOGD); R(30,40,70,50,4,LOGD); HL(18,26,3)
def waterwheel():
    for i in range(8):
        a=i*math.pi/4; L([50,46,50+30*math.cos(a),46+30*math.sin(a)],5,LOGD); C(50+30*math.cos(a),46+30*math.sin(a),6,LOG)
    RING(50,46,26,6,LOG); C(50,46,9,CUT); C(50,46,3.5,CUTD); R(6,78,94,94,8,WATER); waves(86,w=3)
def workshop():
    R(16,48,84,88,6,LOG); roof(8,92,12,52,LOGD); door(22,60,40,88); cog(66,68,11,STONED,LOG); HL(28,56,2.6)
def warehouse():
    R(8,40,92,88,6,LOG); R(4,26,96,46,6,LOGD); R(28,54,72,88,3,CUTD)
    L([28,54,72,88],4,LOG); L([72,54,28,88],4,LOG); HL(16,52,3)
def dock():
    R(6,58,94,92,8,WATER); waves(78,w=4)
    d.polygon(P(20,18,80,18,92,56,8,56),fill=CUT)
    for y in (30,43): L([14,y,86,y],3,CUTD)
    R(12,56,22,84,3,LOGD); R(78,56,88,84,3,LOGD)
def barge():
    R(8,64,92,92,8,WATER); waves(82,w=3)
    d.polygon(P(8,50,92,50,78,76,22,76),fill=LOGD)
    R(20,30,40,50,5,LOG); R(40,24,60,50,5,LOG); R(60,30,80,50,5,LOG); HL(28,40,2.4)
def loggingcamp():
    R(20,52,76,88,10,LOG); E(20,40,76,66,CUT); E(34,48,62,58,CUTD); E(44,51,52,55,CUT)
    L([86,90,72,22],7,LOGD); R(54,12,86,34,7,STONED); R(54,12,70,34,7,STONE); HL(30,74,2.8)
def foragerhut():
    R(18,52,82,88,6,LOG); roof(8,92,12,56,GREEND); roof(26,74,28,54,GREEN); door(38,62,58,88); C(72,76,8,BERRY); HL(30,42,3)
def mudpit():
    E(8,44,92,90,MUDD); E(16,48,84,80,MUD); E(28,56,60,68,MUDL)
    C(36,30,9,MUD); C(60,26,7,MUD); C(72,36,5,MUDL); HL(26,60,3)
def quarrycamp():
    R(10,50,56,88,16,STONE); R(40,38,90,88,18,STONED); R(10,50,40,88,14,STONE); E(18,56,36,70,STONEL); HL(60,52,3)
    L([20,30,60,12],6,LOG); d.polygon(P(54,4,80,18,68,26),fill=STONEL)
def mill():
    d.polygon(P(28,92,72,92,62,44,38,44),fill=CUTD); roof(28,72,22,48,LOGD); R(44,70,56,92,5,INK)
    for a in (45,135,225,315):
        ca,sa=math.cos(math.radians(a)),math.sin(math.radians(a))
        L([50,42,50+30*ca,42+30*sa],5,LOG)
        d.polygon(P(50+14*ca,42+14*sa,50+34*ca,42+34*sa,50+34*ca-9*sa,42+34*sa+9*ca,50+14*ca-9*sa,42+14*sa+9*ca),fill=CUT)
    C(50,42,7,LOGD); HL(40,62,2.8)
def storehouse():
    R(14,30,86,88,12,LOG); R(14,30,86,48,10,LOGD); R(14,62,86,70,0,LOGD); R(44,30,56,88,0,CUTD); HL(26,42,3)
def close_():
    L([28,28,72,72],12,INK); L([72,28,28,72],12,INK)
def locate():
    d.polygon(P(26,44,74,44,50,92),fill=BERRY); C(50,38,27,BERRY); C(50,38,11,CREAM); HL(38,24,3.2)
def alertfood():
    C(50,64,26,BERRY); L([50,40,50,26],5,GREEND); E(52,16,78,32,GREEN); C(42,56,5,PINK); badge()
def alertshelter():
    dome(42,78,36); door(32,62,52,80); R(4,80,80,90,5,GREEND); badge()
def alertidle():
    beaver(38,40,0.95); L([58,58,74,58,58,74,74,74],5,SUN); badge()
def alertstalled():
    cog(44,60,22,STONED,TILES['alert']); badge()
def alertfull():
    R(10,42,70,88,10,LOG); R(10,42,70,56,8,LOGD); R(34,64,46,76,4,CUT)
    C(26,34,9,BERRY); C(46,30,8,GREEN); E(48,30,68,44,STONE); badge()
def alertjoin():
    beaver(56,34,0.95); arrow(6,68,30,68,8,LEAF,11)
def alertwinter():
    snow(46,58,31,5.5,ICE); badge()
def alertwater():
    C(44,66,22,WATER); d.polygon(P(44,26,64,62,24,62),fill=WATER); C(36,64,5,WATERL)
    arrow(76,50,76,86,8,ORANGE,11); badge()
def spring():
    E(8,72,92,100,MUD); L([50,78,50,44],8,GREEND); E(14,24,52,58,GREEN); E(48,16,86,50,'#9ADB78'); HL(28,36)
def summer():
    for i in range(10):
        a=i*math.pi/5; C(50+36*math.cos(a),50+36*math.sin(a),6,SUN)
    C(50,50,25,SUN); HL(40,40,4)
def autumn():
    E(18,18,82,82,ORANGE); d.polygon(P(50,8,66,30,34,30),fill=ORANGE)
    L([50,88,50,30],5,ORANGED); L([50,56,36,44],4,ORANGED); L([50,66,66,50],4,ORANGED); HL(32,34,3)
def winter():
    snow(50,50,38,7,ICE); C(50,50,7,ICE)
def pause():
    R(26,22,44,78,6,LEAF); R(56,22,74,78,6,LEAF)
def play1(): d.polygon(P(32,20,78,50,32,80),fill=LEAF)
def play2():
    for x in (16,46): d.polygon(P(x,24,x+34,50,x,76),fill=LEAF)
def play3():
    for x in (10,36,62): d.polygon(P(x,28,x+26,50,x,72),fill=LEAF)
def social():
    for cx,col in ((30,SUN),(70,'#F7D57A')):
        C(cx,50,21,col); C(cx-8,46,3.5,INK); C(cx+8,46,3.5,INK); ARC((cx-9,44,cx+9,64),25,155,4,INK)
    C(22,60,3.5,PINK); C(78,60,3.5,PINK)
def collect():
    R(16,52,84,88,16,LOG)
    for x in (34,50,66): L([x,60,x-1,80],3,LOGD)
    arrow(50,8,50,34,8,LEAF,11); C(26,44,8,BERRY); C(74,44,8,BERRY); HL(23,41,2)
def haul():
    R(14,36,66,62,10,LOG); logend(62,49,8); L([66,49,88,28],6,CUTD); arrow(12,82,84,82,7,LEAF,11); HL(24,44,2.8)
def craft():
    d.polygon(P(12,52,76,20,84,34,20,66),fill=STONEL)
    for i in range(6): d.polygon(P(22+i*10,58-i*4.8,29+i*10,64-i*4.8,34+i*10,54-i*4.8),fill=STONED)
    L([70,58,86,86],11,LOG); HL(30,46,2.6)
def navcolony():
    dome(34,74,30); door(26,58,42,76); dome(72,78,22); R(6,78,94,90,6,GREEND); HL(24,56,2.8)
def navbuild():
    roof(8,92,10,48,LOG); R(18,48,82,88,4,CUTD); L([18,50,18,88],5,LOG); L([82,50,82,88],5,LOG); L([18,68,82,68],4,LOG)
    L([50,48,50,34],5,LEAF); E(50,22,72,36,GREEN); HL(30,32,2.6)

ICONS=[ # (file, fn, tile kind, label); order = contact sheet order, grouped by GROUPS
 ('logs',logs,'res','Logs'),('branches',branches,'res','Branches'),('bark',bark,'res','Bark'),('plants',plants,'res','Plants'),
 ('mud',mud,'res','Mud'),('stone',stone,'res','Stone'),('baskets',baskets,'res','Baskets'),('rope',rope,'res','Rope'),('tools',tools,'res','Tools'),
 ('food',food,'need','Food'),('shelter',shelter,'need','Shelter'),('rest',rest,'need','Rest'),('mood',mood,'need','Mood'),('social',social,'need','Social'),
 ('collector',collector,'job','Collector'),('builder',builder,'job','Builder'),('hauler',hauler,'job','Hauler'),('specialist',specialist,'job','Specialist'),
 ('dam',dam,'bld','Dam'),('lodge',lodge,'bld','Lodge'),('burrow',burrow,'bld','Burrow'),('foodcache',foodcache,'bld','Food cache'),
 ('scentmound',scentmound,'bld','Scent mound'),('guardpost',guardpost,'bld','Guard post'),('canal',canal,'bld','Canal'),('lock',lock,'bld','Lock'),
 ('waterwheel',waterwheel,'bld','Water wheel'),('workshop',workshop,'bld','Workshop'),('warehouse',warehouse,'bld','Warehouse'),('dock',dock,'bld','Dock'),
 ('barge',barge,'bld','Barge'),('loggingcamp',loggingcamp,'bld','Logging camp'),('foragerhut',foragerhut,'bld','Forager hut'),('mudpit',mudpit,'bld','Mud pit'),
 ('quarrycamp',quarrycamp,'bld','Quarry camp'),('mill',mill,'bld','Mill'),('storehouse',storehouse,'bld','Storehouse'),
 ('assign',assign,'act','Assign'),('fillslot',fillslot,'act','Fill slot'),('movearea',movearea,'act','Move area'),('accept',accept,'act','Accept'),
 ('turnaway',turnaway,'act','Turn away'),('cancel',cancel,'act','Cancel'),('demolish',demolish,'act','Demolish'),('collect',collect,'act','Collect'),
 ('carry',carry,'act','Carry'),('build',build,'act','Build'),('haul',haul,'act','Haul'),('craft',craft,'act','Craft'),
 ('navcolony',navcolony,'act','Colony'),('navbeavers',navbeavers,'act','Beavers'),('navstores',navstores,'act','Stores'),('navbuild',navbuild,'act','Build screen'),
 ('navtech',navtech,'act','Tech'),('navevents',navevents,'act','Events'),('close',close_,'act','Close'),('locate',locate,'act','Locate'),
 ('modewater',modewater,'map','Water'),('modeforest',modeforest,'map','Forest'),('modefood',modefood,'map','Food'),('modework',modework,'map','Work'),
 ('alertfood',alertfood,'alert','Food'),('alertshelter',alertshelter,'alert','Shelter'),('alertidle',alertidle,'alert','Idle'),('alertstalled',alertstalled,'alert','Stalled'),
 ('alertfull',alertfull,'alert','Store full'),('alertjoin',alertjoin,'alert','Joiner'),('alertwinter',alertwinter,'alert','Winter'),('alertwater',alertwater,'alert','Low water'),
 ('spring',spring,'season','Spring'),('summer',summer,'season','Summer'),('autumn',autumn,'season','Autumn'),('winter',winter,'season','Winter'),
 ('pause',pause,'speed','Pause'),('play1',play1,'speed','1x'),('play2',play2,'speed','2x'),('play3',play3,'speed','3x')]
GROUPS=[('Goods','res'),('Needs','need'),('Jobs','job'),('Buildings','bld'),('Actions','act'),('Map modes','map'),('Alerts','alert'),('Seasons','season'),('Speed','speed')]

def font(sz,b=False):
    try: return ImageFont.truetype('C:/Windows/Fonts/segoeui%s.ttf'%('b' if b else ''),sz)
    except Exception: return ImageFont.load_default()

def main():
    os.makedirs(OUT,exist_ok=True)
    for f,fn,kind,_l in ICONS: draw(fn,kind,96).save(os.path.join(OUT,f+'.png'))
    cw=112; per=10; W=per*cw+40
    rows=sum((len([i for i in ICONS if i[2]==k])+per-1)//per for _t,k in GROUPS)
    H=48+len(GROUPS)*30+rows*100+20
    im=Image.new('RGB',(W,H),PANEL); dd=ImageDraw.Draw(im); y=14
    dd.text((20,y),'Icon set (%d icons): each shown at 64 px (left) and 32 px (right)'%len(ICONS),font=font(15,True),fill=INK); y+=34
    for title,kind in GROUPS:
        dd.text((20,y),title,font=font(14,True),fill=INK); y+=24
        row=[i for i in ICONS if i[2]==kind]
        for n,(f,fn,_k,lab) in enumerate(row):
            x=20+(n%per)*cw; yy=y+(n//per)*100
            i64=draw(fn,kind,64); im.paste(i64,(x,yy),i64)
            i32=draw(fn,kind,32); im.paste(i32,(x+68,yy+16),i32)
            dd.text((x,yy+68),lab,font=font(12),fill=DIM)
        y+=((len(row)+per-1)//per)*100+6
    im.crop((0,0,W,y+4)).save(os.path.join(DOCS,'icons_sheet.png'))
main()
