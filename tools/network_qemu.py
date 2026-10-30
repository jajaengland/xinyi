"""Private QEMU checks for Ethernet, DHCP, ICMP, renewal, ring wrap and desktop service."""
import argparse,json,re,shutil,socket,struct,threading,time,tempfile
from pathlib import Path
from smoke import Guest,ROOT
from desktop_controls import SHELL_BUTTON

QEMU=r'C:\Program Files\qemu\qemu-system-i386.exe'
def checksum(b):
    if len(b)%2:b+=b'\0'
    n=sum(struct.unpack('!'+str(len(b)//2)+'H',b))
    while n>>16:n=(n&65535)+(n>>16)
    return ~n&65535
def ip(source,dest,protocol,payload):
    b=bytearray(struct.pack('!BBHHHBBH4s4s',0x45,0,20+len(payload),1,0,64,protocol,0,source,dest));struct.pack_into('!H',b,10,checksum(bytes(b)));return bytes(b)+payload
def echo(kind,sequence,payload,identifier=0x5849):
    b=bytearray(struct.pack('!BBHHH',kind,0,0,identifier,sequence)+payload);struct.pack_into('!H',b,2,checksum(bytes(b)));return bytes(b)
def until(fn,message,timeout=20):
    end=time.monotonic()+timeout
    while not fn():
        assert time.monotonic()<end,message
        time.sleep(.03)

class Peer:
    """Local Ethernet peer; no TAP driver, host configuration or outside network."""
    mac=bytes.fromhex('525500770001');client=bytes.fromhex('525400123456');address=bytes([192,168,77,1]);guest=bytes([192,168,77,10])
    def __init__(self):
        self.server=socket.socket();self.server.bind(('127.0.0.1',0));self.server.listen(1);self.port=self.server.getsockname()[1]
        self.lock=threading.Lock();self.connection=None;self.stop=False;self.errors=[];self.acks=0;self.requests=0;self.discovers=0;self.replies=set();self.renewals=0
        self.thread=threading.Thread(target=self.run,daemon=True);self.thread.start()
    def send(self,b):
        with self.lock:self.connection.sendall(struct.pack('!I',len(b))+b)
    def exact(self,n):
        b=b''
        while len(b)<n:
            part=self.connection.recv(n-len(b))
            if not part:raise EOFError()
            b+=part
        return b
    def run(self):
        try:
            self.connection,_=self.server.accept()
            while not self.stop:
                n=struct.unpack('!I',self.exact(4))[0];assert 14<=n<=1536;self.packet(self.exact(n))
        except (EOFError,OSError):pass
        except BaseException as e:self.errors.append(repr(e))
    def packet(self,b):
        assert b[6:12]==self.client
        if b[12:14]==b'\x08\x06':
            a=b[14:42]
            if a[6:8]==b'\0\1' and a[24:28]==self.address:
                reply=b'\0\1\x08\0\6\4\0\2'+self.mac+self.address+self.client+self.guest
                self.send(self.client+self.mac+b'\x08\x06'+reply)
            return
        if b[12:14]!=b'\x08\0':return
        header=b[14:34];assert checksum(header)==0
        body=b[34:14+struct.unpack_from('!H',header,2)[0]]
        if header[9]==17:
            assert body[:4]==struct.pack('!HH',68,67)
            request=body[8:];assert request[0:3]==b'\1\1\6' and request[28:34]==self.client
            opts={};at=240
            while at<len(request):
                code=request[at];at+=1
                if code==255:break
                if code==0:continue
                length=request[at];at+=1;opts[code]=request[at:at+length];at+=length
            kind=opts[53][0]
            if kind==1:self.discovers+=1
            elif kind==3:
                self.requests+=1
                if request[12:16]==self.guest:
                    if header[16:20]==self.address:self.renewals+=1
                    assert 50 not in opts and 54 not in opts
            else:raise AssertionError('unexpected DHCP type')
            reply=bytearray(240);reply[:3]=b'\2\1\6';reply[4:8]=request[4:8];reply[16:20]=self.guest;reply[28:34]=self.client;reply[236:240]=bytes([99,130,83,99])
            reply+=bytes([53,1,2 if kind==1 else 5,54,4])+self.address+bytes([1,4,255,255,255,0,3,4])+self.address+bytes([6,4])+self.address+bytes([51,4])+struct.pack('!I',10)+b'\xff'
            udp=struct.pack('!HHHH',67,68,8+len(reply),0)+reply
            dest=self.guest if request[12:16]==self.guest else bytes([255]*4)
            self.send(self.client+self.mac+b'\x08\0'+ip(self.address,dest,17,udp))
            if kind==3:self.acks+=1
        elif header[9]==1:
            assert checksum(body)==0
            if body[0]==0 and body[4:6]==b'XI':self.replies.add(struct.unpack_from('!H',body,6)[0])
            elif body[0]==8:self.send(self.client+self.mac+b'\x08\0'+ip(self.address,self.guest,1,echo(0,struct.unpack_from('!H',body,6)[0],body[8:],struct.unpack_from('!H',body,4)[0])))
    def probe(self,sequence):self.send(self.client+self.mac+b'\x08\0'+ip(self.address,self.guest,1,echo(8,sequence,bytes(range(200))*2)))
    def close(self):
        self.stop=True
        if self.connection:
            try:self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:pass
            self.connection.close()
        self.server.close();self.thread.join(timeout=2)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--kernel',type=Path,required=True);p.add_argument('--disk',type=Path,required=True);a=p.parse_args()
    out=ROOT/'target/smoke';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='network-',dir=out) as tmp:
        d=Path(tmp)
        g=Guest(QEMU,a.kernel,d,attach_disk=False)
        try:g.command('net','Networking disabled');g.command('net experimental-enable','no supported RTL8139');g.command('echo still usable','still usable')
        finally:g.close()
        g=Guest(QEMU,a.kernel,d,attach_disk=False,extra_args=['-netdev','user,id=net0','-device','rtl8139,netdev=net0,id=nic0'])
        try:
            g.command('net','Networking disabled');g.command('net experimental-enable','enabled for this boot only')
            log=lambda:(d/'debug.log').read_text(errors='replace')
            until(lambda:'Net: bound ip=10.0.2.15' in log(),'DHCP failed: '+log())
            screen=g.command('net','IPv4:');assert 'Gateway: 10.0.2.2' in screen and 'DNS: 10.0.2.3' in screen
            base=int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])
            g.command('ping 10.0.2.2','Pinging');until(lambda:'Net: ping complete sent=4 received=4 lost=0' in log(),'QEMU router ping failed')
            g.command('ping','4 sent, 4 received, 0 lost');g.command('ping 999.1.2.3','Usage:');g.command('ping 10.0.2.255','unicast destination')
            g.call('set_link',{'name':'nic0','up':False});until(lambda:'Net: link down' in log(),'disconnect');g.command('net','link down')
            g.call('set_link',{'name':'nic0','up':True});until(lambda:log().count('Net: bound ip=10.0.2.15')>=2,'reconnect DHCP')
            g.command('ping 10.0.2.254','Pinging');until(lambda:'Net: ping complete sent=0 received=0' in log(),'ARP timeout')
            count=log().count('Net: bound ip=10.0.2.15');g.command('net dhcp','DHCP address request started');until(lambda:log().count('Net: bound ip=10.0.2.15')>count,'manual DHCP')
            assert int(re.search(r'Free now: +(\d+)',g.command('frames','Free now:'))[1])==base,'Network retries leaked frames'
        finally:g.close();shutil.copyfile(d/'debug.log',out/'network-slirp-debug.log')
        peer=Peer();g=None
        try:
            g=Guest(QEMU,a.kernel,d,attach_disk=False,extra_args=['-netdev',f'socket,id=net0,connect=127.0.0.1:{peer.port}','-device','rtl8139,netdev=net0,id=nic0,mac=52:54:00:12:34:56'])
            time.sleep(.3);assert peer.discovers==0,'DHCP sent while disabled'
            g.command('net experimental-enable','enabled for this boot only')
            until(lambda:peer.acks>=1,'private DHCP handshake')
            until(lambda:'Net: bound ip=192.168.77.10' in log(),'private lease binding')
            for i in range(48):peer.probe(i);time.sleep(.025)
            until(lambda:len(peer.replies)>=48,'receive-ring wrap dropped echo replies')
            until(lambda:peer.renewals>=1 and peer.acks>=2,'short DHCP lease did not renew',timeout=12)
            g.command('net','IPv4: 192.168.77.10');assert 'DHCP lease: 10 seconds' in g.screen()
            # A burst can overrun the small hardware ring; the driver must recover.
            for _ in range(256):peer.send(peer.client+peer.mac+b'\x88\xb5'+bytes(400))
            time.sleep(.3);peer.probe(200);until(lambda:200 in peer.replies,'receive recovery after burst')
            g.command('ping 192.168.77.1','Pinging');until(lambda:'Net: ping complete sent=4 received=4 lost=0' in log(),'private peer ping')
            g.command('net off','Networking disabled.');acks=peer.acks;count=peer.requests;time.sleep(.3);peer.probe(201);time.sleep(.3);assert 201 not in peer.replies and peer.requests==count
            g.command('ping 192.168.77.1','Networking disabled');g.command('net experimental-enable','enabled for this boot only');until(lambda:peer.acks>acks,'re-enable DHCP')
            assert not peer.errors,peer.errors
        finally:
            if g:g.close()
            peer.close();shutil.copyfile(d/'debug.log',out/'network-private-debug.log')
        # Run the same stack from BIOS disk boot while independent GUI apps are alive.
        image=d/'desktop.img';shutil.copyfile(a.disk,image)
        g=Guest(QEMU,None,d,disk=image,extra_args=['-netdev','user,id=net0','-device','rtl8139,netdev=net0,id=nic0'])
        try:
            g.command('net','Networking disabled');g.command('net experimental-enable','enabled for this boot only')
            g.type('desktop\n');until(lambda:'Desktop scheduler: ready' in log(),'desktop')
            for query in ['counter','terminal']:
                marker=len(log());g.key('ctrl-esc');g.type(query+'\n');until(lambda:'Multiwindow: open pid=' in log()[marker:],'window '+query)
            g.type('net\n');time.sleep(.3);g.type('ping 10.0.2.2\n');until(lambda:'Net: ping complete sent=4 received=4 lost=0' in log(),'Terminal ping')
            g.call('screendump',{'filename':str(out/'network-desktop.png'),'format':'png'});g.key('ctrl-q');time.sleep(.3);g.key('alt-f4');time.sleep(.3)
            # Closing the terminal while a ping is pending must not target a dead session.
            g.key('ctrl-esc');g.type('terminal\n');until(lambda:log().count('PTY: open')>=2,'second terminal')
            g.type('ping 10.0.2.254\n');g.key('ctrl-q');until(lambda:log().count('Net: ping complete')>=2,'abandoned terminal ping')
            assert 'Xinyi panic:' not in log()
        finally:g.close();shutil.copyfile(d/'debug.log',out/'network-desktop-debug.log')
    print('PASS networking: absent adapter, DHCP/NAT, ARP, four-reply ping, timeouts, link reconnect, frame balance, private short lease renewal, ring wrap/burst recovery, incoming echo, BIOS desktop concurrency and closed-terminal cleanup')

if __name__=='__main__':main()
