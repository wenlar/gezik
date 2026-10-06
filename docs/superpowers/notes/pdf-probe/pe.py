import struct,sys
def exports(p):
    b=open(p,'rb').read()
    pe=struct.unpack_from('<I',b,0x3c)[0]
    nsec=struct.unpack_from('<H',b,pe+6)[0]; optsz=struct.unpack_from('<H',b,pe+20)[0]
    opt=pe+24; magic=struct.unpack_from('<H',b,opt)[0]
    dd=opt+(112 if magic==0x20b else 96)
    erva,esz=struct.unpack_from('<II',b,dd)
    secs=[struct.unpack_from('<8sIIII',b,pe+24+optsz+i*40) for i in range(nsec)]
    def off(rva):
        for n,vs,va,rs,ro in secs:
            if va<=rva<va+max(vs,rs): return rva-va+ro
    e=off(erva); nn,af,an=struct.unpack_from('<III',b,e+24)[0],0,struct.unpack_from('<I',b,e+32)[0]
    names=[]
    for i in range(nn):
        r=struct.unpack_from('<I',b,off(an)+4*i)[0]; o=off(r); names.append(b[o:b.index(b'\0',o)].decode())
    return set(names)
a=exports(sys.argv[1]); b2=exports(sys.argv[2])
print(len(a),len(b2)); print('only in second:',sorted(b2-a)[:40]); print('only in first:',sorted(a-b2)[:40])
