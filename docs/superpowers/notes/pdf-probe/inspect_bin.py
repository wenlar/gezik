import re,struct,sys
def elf(p):
    b=open(p,'rb').read()
    g=sorted(set(re.findall(rb'GLIBC_([0-9.]+)',b)),key=lambda v:[int(x) for x in v.split(b'.')])
    libs=sorted(set(re.findall(rb'(lib[a-z0-9_+\-]+\.so(?:\.[0-9]+)*)\x00',b)))
    print(p,'glibc max',g[-1:] ,'needed-ish',[l.decode() for l in libs][:15])
def macho(p):
    b=open(p,'rb').read()
    magic,cpu,sub,ft,ncmds,size,flags,res=struct.unpack_from('<IiiIIIII',b,0)
    off=32; out=[]
    for i in range(ncmds):
        cmd,cs=struct.unpack_from('<II',b,off)
        if cmd==0x1d: 
            dataoff,datasize=struct.unpack_from('<II',b,off+8)
            sig=b[dataoff:dataoff+datasize]
            # find CMS / adhoc: look for team id or 'adhoc' flag in CodeDirectory
            cd=sig.find(b'\xfa\xde\x0c\x02')
            fl=struct.unpack_from('>I',sig,cd+12)[0] if cd>=0 else None
            has_cms=b'Apple Certification' in sig or b'Developer ID' in sig
            out.append(('LC_CODE_SIGNATURE',datasize,'cdflags=%#x'%fl,'adhoc' if fl and fl&2 else 'not-adhoc','cert' if has_cms else 'no-cert', 'linker-signed' if fl and fl&0x20000 else ''))
        if cmd in (0xc,0x8000001f):
            no=struct.unpack_from('<I',b,off+8)[0]; out.append(('dylib',b[off+no:off+cs].split(b'\0')[0].decode()))
        if cmd==0x32:
            plat,minos=struct.unpack_from('<II',b,off+8); out.append(('minos','%d.%d'%(minos>>16,(minos>>8)&0xff)))
        off+=cs
    print(p,cpu); [print('  ',o) for o in out]
for p in sys.argv[1:]:
    (macho if p.endswith('dylib') else elf)(p)
