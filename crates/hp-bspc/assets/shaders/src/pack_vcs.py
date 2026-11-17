import lzma, struct, sys, zlib
code = open(sys.argv[1], "rb").read()
inner = struct.pack("<ii", 0, len(code)) + code
comp = lzma.compress(inner, format=lzma.FORMAT_ALONE, filters=[{"id": lzma.FILTER_LZMA1, "dict_size": 1 << 16, "lc": 3, "lp": 0, "pb": 2}])
props = comp[:5]
data = comp[13:]
block = b"LZMA" + struct.pack("<II", len(inner), len(data)) + props + data
start = 28 + 16 + 4
end = start + 4 + len(block) + 4
crc = zlib.crc32(code) & 0xffffffff
out = struct.pack("<6iI", 6, 1, 1, 0, 0, 2, crc)
out += struct.pack("<II", 0, start) + struct.pack("<II", 0xffffffff, end) + struct.pack("<i", 0)
out += struct.pack("<I", 0x40000000 | len(block)) + block + struct.pack("<I", 0xffffffff)
assert len(out) == end
open(sys.argv[2], "wb").write(out)
