import sys,zlib
print("%08x" % (zlib.crc32(sys.stdin.buffer.read()) & 0xffffffff))
