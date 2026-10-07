import socket
l=socket.socket(socket.AF_INET6);l.bind(('::',0));l.listen(4);p=l.getsockname()[1]
print("V6ONLY default", l.getsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY))
for host,fam in (('127.0.0.1',socket.AF_INET),('::1',socket.AF_INET6)):
    c=socket.socket(fam);c.connect((host,p));s,_=l.accept();print(host,'->',s.getpeername()[:2]);s.close();c.close()
