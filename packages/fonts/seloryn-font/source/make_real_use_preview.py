from pathlib import Path
import re, io
from PIL import Image,ImageDraw,ImageFont
import cairosvg
P=Path(__file__).resolve().parent.parent
fonts=P/'fonts'
W,H=1810,1700
BG='#09120f'; PANEL='#0c1812'; MUTED='#a1b9ac'; TEXT='#edf8f0'; GREEN='#34cca7'; DEEP='#148874'; LINE='#2b3e34'
img=Image.new('RGB',(W,H),BG);d=ImageDraw.Draw(img)
def family(weight,size):return ImageFont.truetype(str(fonts/f'seloryn-font-{weight}.ttf'),size)
def system(size,weight=False):return ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSans'+('-Bold' if weight else '')+'.ttf',size)
def t(x,y,txt,font,color=TEXT):d.text((x,y),txt,font=font,fill=color,anchor='lt')
def centertext(x,y,txt,font,color=TEXT):d.text((x,y),txt,font=font,fill=color,anchor='mt')
def rule(y):d.line((100,y,1710,y),fill=LINE,width=2)
def small(x,y,txt):t(x,y,txt,system(18,True),MUTED)
logo=(P/'seloryn-symbol.svg').read_bytes()
mark=Image.open(io.BytesIO(cairosvg.svg2png(bytestring=logo,output_width=108,output_height=108))).convert('RGBA')
img.paste(mark,(80,50),mark)
t(205,72,'Seloryn',family('bold',52))
t(1262,93,'EXEMPLO REAL',system(16,True),MUTED)
d.rounded_rectangle((1491,67,1702,123),radius=27,outline=DEEP,width=2)
centertext(1596,80,'FONT 02',family('light',29),GREEN)
rule(188)
small(98,244,'IDENTIDADE TIPOGRÁFICA   /   EN + PT-BR')
t(88,300,'Seloryn',family('bold',239),GREEN)
t(104,541,'A linguagem tem sua própria forma.',family('light',65),TEXT)
t(102,643,'DUAS CORRENTES DE CURVA     /     TERMINAIS REDONDOS     /     ABERTURAS INTENCIONAIS',system(16,True),MUTED)
rule(708)
small(99,748,'01 / APLICAÇÃO EM UM PROGRAMA SELORYN')
t(100,798,'Código verificável.',family('bold',65),TEXT)
# Code panel
px,py,pw,ph=99,900,1611,474
d.rounded_rectangle((px,py,px+pw,py+ph),radius=14,fill=PANEL,outline=LINE,width=2)
d.line((px,971,px+pw,971),fill=LINE,width=2)
t(128,922,'tensor-matmul.sely',family('bold',30),TEXT)
t(1260,928,'ARQUIVO REAL / REPO',system(16,True),GREEN)
# side border
d.line((1225,971,1225,1374),fill=LINE,width=2)
CODE=[
 'fn main() -> Tensor[F32]:',
 '    let left: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2])',
 '    let right: Tensor[F32] = tensor_f32([5.0, 6.0, 7.0, 8.0], [2, 2])',
 '    return matmul(left, right)',
]
# Simple literal lexical coloring without altering code characters
font=ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf',21);kwfont=ImageFont.truetype('/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf',21)
pat=re.compile(r'\b(fn|let|return)\b|\b(Tensor|F32)\b|\b(\d+(?:\.\d+)?)\b')
for i,line in enumerate(CODE):
 y=1006+i*73; x=130
 for m in pat.finditer(line):
  a=line[:m.start()]; prev=line[:m.start()]
  # draw preceding untouched raw fragment using cursor
 cpos=0;x=132
 for m in pat.finditer(line):
  pre=line[cpos:m.start()]
  t(x,y,pre,font,TEXT);x+=d.textlength(pre,font=font)
  tok=m.group(0)
  col=GREEN if m.group(1) else ('#a8d7c6' if m.group(2) else '#f1dec1')
  ft=kwfont if m.group(1) else font
  t(x,y,tok,ft,col);x+=d.textlength(tok,font=ft)
  cpos=m.end()
 pre=line[cpos:];t(x,y,pre,font,TEXT)
# real code side
t(1262,1012,'EXECUÇÃO',system(17,True),GREEN)
t(1258,1053,'Duas matrizes.',family('bold',39),TEXT)
t(1258,1104,'Uma operação.',family('bold',37),TEXT)
t(1262,1182,'Trecho literal do projeto',system(17),MUTED)
t(1262,1213,'público Seloryn.',system(17),MUTED)
t(1262,1319,'SEM RESULTADOS SIMULADOS',system(13,True),MUTED)
# Lower specimen segment
rule(1434)
small(99,1484,'02 / TIPOGRAFIA APLICADA  ·  LIGHT 300 + BOLD 700')
t(100,1525,'á é í ó ú ç ã õ',family('light',47),TEXT)
t(868,1525,'Tensor[F32] 0123456789',family('bold',43),GREEN)
small(100,1618,'DEMONSTRAÇÃO DE INTERFACE  ·  CÓDIGO RETIRADO DE examples/tensors/tensor-matmul.sely')
img.save(P/'previews'/'seloryn-font-real-use.png',optimize=True)
print(P/'previews'/'seloryn-font-real-use.png')
