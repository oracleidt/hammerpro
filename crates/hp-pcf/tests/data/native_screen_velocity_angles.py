from pathlib import Path
import ctypes, hashlib, json, math, random, struct, sys
import pefile, unicorn, capstone
from unicorn.x86_const import *

w=Path(__file__).parent
raw=Path('D:/SteamLibrary/steamapps/common/GarrysMod/bin/win64/client.dll').read_bytes()
pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
cs=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64)
print('Native atan2 entry:',[(i.mnemonic,i.op_str)for i in cs.disasm(pe.get_data(0x5fd6e0,16),0x1805fd6e0)],flush=True)
u=unicorn.Uc(unicorn.UC_ARCH_X86,unicorn.UC_MODE_64)
u.mem_map(base,pe.OPTIONAL_HEADER.SizeOfImage);u.mem_write(base,pe.get_memory_mapped_image())
if '--cpu3' in sys.argv:
    u.mem_write(base+0xaaa0d8,struct.pack('<I',3))
u.mem_map(0x10000000,0x10000);u.mem_map(0x21000000,0x10000)
u.reg_write(UC_X86_REG_MXCSR,0x1f80)
f=lambda v:struct.unpack('<f',struct.pack('<f',v))[0]
wf=lambda at,v:u.mem_write(at,struct.pack('<f',v))
wi=lambda at,v:u.mem_write(at,struct.pack('<I',v))
wq=lambda at,v:u.mem_write(at,struct.pack('<Q',v))
obj=0x21000000;coll=obj+0x1000;record=obj+0x4000
current=obj+0x5000;previous=obj+0x5100;angle=obj+0x5200;stack=0x10008000
crt=ctypes.CDLL('ucrtbase.dll');crt.atan2.argtypes=[ctypes.c_double,ctypes.c_double];crt.atan2.restype=ctypes.c_double
def native_atan2(emu,address,size,data):
    read=lambda reg:struct.unpack('<d',struct.pack('<Q',emu.reg_read(reg)&0xffffffffffffffff))[0]
    value=crt.atan2(read(UC_X86_REG_XMM0),read(UC_X86_REG_XMM1))
    emu.reg_write(UC_X86_REG_XMM0,struct.unpack('<Q',struct.pack('<d',value))[0])
    sp=emu.reg_read(UC_X86_REG_RSP)
    ret=struct.unpack('<Q',emu.mem_read(sp,8))[0]
    emu.reg_write(UC_X86_REG_RSP,sp+8);emu.reg_write(UC_X86_REG_RIP,ret)
if '--native-math' not in sys.argv:
    u.hook_add(unicorn.UC_HOOK_CODE,native_atan2,begin=0x1805fd6e0,end=0x1805fd6e0)
randomizer=random.Random(117911)
cases=[]
for index in range(360):
    position=[f(randomizer.uniform(-100000,100000))for _ in range(3)]
    prev=position[:] if index%20==0 else[f(p+randomizer.uniform(-10,10))for p in position]
    yaw=randomizer.uniform(-math.pi,math.pi);pitch=randomizer.uniform(-1.3,1.3)
    right=[f(math.sin(yaw)),f(-math.cos(yaw)),0.]
    up=[f(-math.sin(pitch)*math.cos(yaw)),f(-math.sin(pitch)*math.sin(yaw)),f(math.cos(pitch))]
    rows=[v+[f(randomizer.uniform(-1000,1000))]for v in [right,up]]
    forward=f(randomizer.choice([-90.,0.,90.,180.,17.3,-1000.,randomizer.uniform(-10000,10000)]))
    u.mem_write(obj,bytes(0x6000))
    for offset,ptr in [(0x1d20,current),(0x1d30,previous),(0x1d40,angle)]:wq(coll+offset,ptr)
    for offset,stride in [(0x1f20,48),(0x1f30,48),(0x1f40,16)]:wq(coll+offset,stride)
    for axis in range(3):wf(current+axis*16,position[axis]);wf(previous+axis*16,prev[axis])
    wf(angle,0.333);wf(obj+0x9c,forward);wi(stack+0x160,1)
    for i,value in enumerate(rows[0]+rows[1]):wf(stack+0x40+i*4,value)
    for reg,value in [(UC_X86_REG_R14,obj),(UC_X86_REG_RBX,coll),(UC_X86_REG_RAX,record+12),(UC_X86_REG_RSP,stack)]:u.reg_write(reg,value)
    u.emu_start(0x180450ac9,0x180450d6c,count=10000)
    result=struct.unpack('<f',u.mem_read(angle,4))[0]
    cases.append(dict(position=position,previous=prev,view=rows,forward=forward,expected=result))
report=dict(source_sha256=hashlib.sha256(raw).hexdigest(),helper='0x180450a50',arithmetic_entry='0x180450ac9',atan2='installed native math' if '--native-math' in sys.argv else 'ucrtbase.atan2 substitution',cpu_dispatch=3 if '--cpu3' in sys.argv else 0,cases=cases)
(w/('native-screen-velocity-angles'+('-full' if '--native-math' in sys.argv else '')+('-cpu3' if '--cpu3' in sys.argv else '')+'.json')).write_text(json.dumps(report,separators=(',',':'))+'\n',encoding='utf-8')
print('Native screen-velocity angle references:',len(cases),'first',cases[0],flush=True)
