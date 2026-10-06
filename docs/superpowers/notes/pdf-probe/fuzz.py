import random, subprocess, collections, sys, os
random.seed(1)
src = [open(f,'rb').read() for f in ['out/merged.pdf','out/fonts.pdf','out/extract.pdf']]
os.makedirs('out/fuzz', exist_ok=True)
res = collections.Counter(); crashes = []
N = int(sys.argv[1])
for i in range(N):
    b = bytearray(random.choice(src))
    for _ in range(random.randint(1, 40)):
        op = random.random(); p = random.randrange(len(b))
        if op < 0.6: b[p] = random.randrange(256)
        elif op < 0.8: del b[p:p+random.randint(1,64)]
        else: b[p:p] = bytes(random.choice(b'0123456789 <>/[]()RobjstreamendFlateDecode') for _ in range(random.randint(1,16)))
    f = f'out/fuzz/{i}.pdf'; open(f,'wb').write(b)
    try:
        r = subprocess.run([os.path.abspath('target/release/fuzzone.exe'), os.path.abspath('../pdfium/win-x64/bin'), f], capture_output=True, timeout=20)
        code = r.returncode
    except subprocess.TimeoutExpired:
        code = 'timeout'
    if code == 0:
        res['handled'] += 1; os.remove(f)
    else:
        res[f'exit {code}'] += 1; crashes.append((f, code))
print(dict(res)); print(crashes[:10])
