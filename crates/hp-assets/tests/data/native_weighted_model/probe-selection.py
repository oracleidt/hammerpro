from pathlib import Path
import ctypes as c,os,struct,json,hashlib,sys
import pefile,unicorn
from unicorn.x86_const import *
root=Path(sys.argv[1]);cookie=os.add_dll_directory(str(root));dll=c.CDLL(str(root/'vstdlib.dll'));dll.RandomSeed.argtypes=[c.c_int];dll.RandomInt.argtypes=[c.c_int,c.c_int];dll.RandomInt.restype=c.c_int
raw=(root/'client.dll').read_bytes();assert hashlib.sha256(raw).hexdigest()==json.loads(Path(__file__).with_name('native-selection.json').read_text(encoding='utf-8'))['source_sha256'], 'native ABI reference requires this exact installed DLL';pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
u=unicorn.Uc(unicorn.UC_ARCH_X86,unicorn.UC_MODE_64);u.mem_map(base,pe.OPTIONAL_HEADER.SizeOfImage);u.mem_write(base,pe.get_memory_mapped_image());u.mem_map(0x10000000,0x10000);u.mem_map(0x21000000,0x10000)
stop=0x10000000;callback=stop+0x100;stack=stop+0x8008;mapping=0x21000000;tuples=mapping+0x1000;buckets=mapping+0x2000;entry=mapping+0x3000;hdr=mapping+0x4000
wi=lambda at,v:u.mem_write(at,struct.pack('<I',v&0xffffffff));wq=lambda at,v:u.mem_write(at,struct.pack('<Q',v))
def ret(value):
 u.reg_write(UC_X86_REG_RAX,value&0xffffffffffffffff);rsp=u.reg_read(UC_X86_REG_RSP);u.reg_write(UC_X86_REG_RIP,struct.unpack('<Q',u.mem_read(rsp,8))[0]);u.reg_write(UC_X86_REG_RSP,rsp+8)
def hook(u,address,size,data):
 if address==base+0x2d2b80:ret(1) # Already-loaded model/cache identity validation only.
 elif address==callback:ret(dll.RandomInt(c.c_int(u.reg_read(UC_X86_REG_ECX)).value,c.c_int(u.reg_read(UC_X86_REG_EDX)).value))
u.hook_add(unicorn.UC_HOOK_CODE,hook)
wq(base+0x632990,callback);wi(base+0x876510,-1)
cases=[]
for weights in [[1,3,6],[0,3,6],[-1,-3,-6],[0,0,0],[1],[7]]:
 dll.RandomSeed(1);u.mem_write(mapping,bytes(0x5000));wq(mapping,tuples);wq(mapping+0x10,buckets);wi(mapping+0x20,1);wq(buckets,entry);wi(buckets+0x10,1);wi(entry,1);wi(entry+4,0);wi(entry+8,len(weights));wi(entry+12,sum(abs(w)for w in weights))
 for i,w in enumerate(weights):u.mem_write(tuples+i*4,struct.pack('<hh',i+1,max(abs(w),1)))
 results=[]
 for i in range(512):
  for reg,value in [(UC_X86_REG_RCX,mapping),(UC_X86_REG_RDX,hdr),(UC_X86_REG_R8,1),(UC_X86_REG_R9,0xffffffff),(UC_X86_REG_RSP,stack)]:u.reg_write(reg,value)
  wq(stack,stop);u.emu_start(base+0x167eb0,stop,count=10000);results.append(c.c_int(u.reg_read(UC_X86_REG_EAX)).value)
 cases.append({'weights':weights,'seed':1,'expected':results})
 print(weights,'512 selections', {n:results.count(n)for n in set(results)})
out={'source_sha256':hashlib.sha256(raw).hexdigest(),'method':'client RVA167eb0 native activity selector, model-cache validation supplied, actual native vstdlib RandomInt callback, actual tuple scan/hash/ticket subtraction executed','cases':cases}
Path(__file__).with_name('native-selection.json').write_text(json.dumps(out,indent=2),encoding='utf-8')
