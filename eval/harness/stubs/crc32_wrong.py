import sys,zlib
print("%08X" % (zlib.crc32(sys.stdin.buffer.read()) & 0xffffffff))
