"""Small PSD with a clipped layer, a group, masks, layer styles and point text."""
from pathlib import Path
from struct import pack

def u(n): return pack(">I", n)
def uni(s): return u(len(s))+s.encode("utf-16-be")
def key(s): return u(0 if len(s)==4 else len(s))+s.encode()
def desc(cls, fields):
    return uni("")+key(cls)+u(len(fields))+b"".join(key(k)+t+v for k,t,v in fields)
def tag(k,p): return b"8BIM"+k.encode()+u(len(p))+p+bytes(len(p)%2)
def knot(x,y):
    return pack(">Hiiiiii",1,*([int(y*2**24),int(x*2**24)]*3))
def layer(name, index, x, y, w, h, *, tags=b"", clip=0, hidden=False, mask=None):
    bounds=pack(">iiii",y,x,y+h,x+w)
    planes=[(i,b"\0\0"+bytes([v])*w*h) for i,v in [(0,120),(1,10),(2,90),(-1,230)]] if w*h else []
    m=b""
    if mask is not None:
        planes.append((-2,b"\0\0"+mask))
        m=bounds+bytes(4)
        if b"vmsk" in tags:
            planes[-1]=(-2,b"\0\0"+bytes([255])*len(mask))
            planes.append((-3,b"\0\0"+mask))
            m=bounds+bytes([0,8,0,0])+bounds
    pascal=bytes([len(name)])+name.encode()
    extra=u(len(m))+m+u(0)+pascal+bytes(-len(pascal)%4)+tag("lyid",u(index))+tags
    record=bounds+pack(">H",len(planes))+b"".join(pack(">hI",i,len(v)) for i,v in planes)
    return record+b"8BIMnorm"+bytes([255,clip,2 if hidden else 0,0])+u(len(extra))+extra,b"".join(v for _,v in planes)

shadow=desc("DrSh",[("enab",b"bool",b"\x01"),("Dstn",b"UntF",b"#Pxl"+pack(">d",17.5))])
styles=tag("lmfx",u(0)+u(16)+desc("null",[("dropShadowMulti",b"VlLs",u(1)+b"Objc"+shadow)]))
vector=tag("vmsk",u(3)+u(0)+pack(">HHh20x",0,4,1)+b"".join(knot(x,y) for x,y in [(0,0),(.5,0),(.5,.5),(0,.5)]))
engine=b"<< /EngineDict << /StyleRun << /RunArray [ << /StyleSheet << /StyleSheetData << /Font 0 /FontSize 18 /FillColor << /Values [1 1 0.5 0] >> >> >> >> ] >> >> /ResourceDict << /FontSet [ << /Name (TestPS) >> ] >> >>"
text=pack(">H6dHI",1,1,0,0,1,2,3,50,16)+desc("TxLr",[("Txt ",b"TEXT",uni("Hello")),("EngineData",b"tdta",u(len(engine))+engine)])+pack(">HI",1,16)+desc("warp",[])+bytes(16)
layers=[layer("End",1,0,0,0,0,tags=tag("lsct",u(3))),
        layer("Base",2,1,1,3,2,tags=styles+vector,mask=bytes([0,128,255,255,128,0])),
        layer("Clip",3,2,1,1,1,clip=1,hidden=True),
        layer("Group",4,1,1,3,2,tags=tag("lsct",u(1)),mask=bytes([255,255,255,128,0,255])),
        layer("Title",5,2,2,2,1,tags=tag("TySh",text))]
info=pack(">h",-len(layers))+b"".join(r for r,_ in layers)+b"".join(p for _,p in layers)
info+=bytes(-len(info)%4)
section=u(len(info))+info+u(0)
hdr=b"8BPS"+pack(">H6xHIIHH",1,4,6,8,8,3)
Path(__file__).with_name("art.psd").write_bytes(hdr+bytes(8)+u(len(section))+section+bytes(2+4*6*8))
