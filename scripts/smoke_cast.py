#!/usr/bin/env python3
"""Exercise a running receiver through SSDP, SOAP, and GENA (no phone required)."""
import argparse
import http.server
import pathlib
import socket
import threading
import time
import urllib.error
import urllib.request
import urllib.parse
import xml.etree.ElementTree as ET
from xml.sax.saxutils import escape

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--receiver", required=True, help="http://LAN_IP:5200")
parser.add_argument("--media", default="https://raw.githubusercontent.com/kohsine/libmpv2-rs/master/test-data/jellyfish.mp4")
parser.add_argument("--local-media", type=pathlib.Path, help="Serve a local diagnostic video over HTTP")
args = parser.parse_args()
base = args.receiver.rstrip("/")
ip = urllib.parse.urlparse(base).hostname
notifications = []
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_NOTIFY(self):
        body = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        ET.fromstring(body)
        notifications.append((self.headers.get('SID'), int(self.headers.get('SEQ', -1)), body))
        self.send_response(200); self.end_headers()
    def do_GET(self):
        if self.path != '/media.mp4' or not args.local_media:
            self.send_error(404); return
        size = args.local_media.stat().st_size
        start, end = 0, size - 1
        requested_range = self.headers.get('Range')
        if requested_range:
            begin, finish = requested_range.removeprefix('bytes=').split('-', 1)
            start = int(begin or 0); end = min(int(finish or end), size - 1)
        if start >= size:
            self.send_error(416); return
        self.send_response(206 if requested_range else 200)
        self.send_header('Content-Type', 'video/mp4')
        self.send_header('Accept-Ranges', 'bytes')
        self.send_header('Content-Length', str(end - start + 1))
        if requested_range: self.send_header('Content-Range', f'bytes {start}-{end}/{size}')
        self.end_headers()
        try:
            with args.local_media.open('rb') as media:
                media.seek(start)
                remaining = end - start + 1
                while remaining:
                    chunk = media.read(min(65536, remaining))
                    if not chunk: break
                    self.wfile.write(chunk); remaining -= len(chunk)
        except (BrokenPipeError, ConnectionResetError):
            pass
server = http.server.ThreadingHTTPServer(('0.0.0.0', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
callback = f'http://{ip}:{server.server_port}/events'
media = f'http://{ip}:{server.server_port}/media.mp4' if args.local_media else args.media

def request(path, method='GET', headers=None, body=None):
    req = urllib.request.Request(base + path, data=body, headers=headers or {}, method=method)
    return urllib.request.urlopen(req, timeout=8)

def action(service, name, **arguments):
    inner = ''.join(f'<{key}>{escape(str(value))}</{key}>' for key, value in arguments.items())
    body = f'<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:{name} xmlns:u="urn:schemas-upnp-org:service:{service}:1">{inner}</u:{name}></s:Body></s:Envelope>'
    with request('/control/' + service, 'POST', {'SOAPAction': f'"urn:schemas-upnp-org:service:{service}:1#{name}"', 'Content-Type': 'text/xml'}, body.encode()) as response:
        doc = ET.fromstring(response.read())
        return {node.tag.rsplit('}', 1)[-1]: node.text or '' for node in doc.iter() if len(node) == 0}

def wait_state(expected, timeout=15):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = action('AVTransport', 'GetTransportInfo', InstanceID=0)
        if last.get('CurrentTransportState') == expected: return last
        time.sleep(.25)
    raise AssertionError(f'Expected {expected}, got {last}')

try:
    with request('/description.xml') as response:
        device = ET.fromstring(response.read())
    ns = {'d': 'urn:schemas-upnp-org:device-1-0'}
    name = device.find('.//d:friendlyName', ns).text
    assert device.find('.//d:deviceType', ns).text.endswith(':MediaRenderer:1')
    for service in ['AVTransport', 'RenderingControl', 'ConnectionManager']:
        with request('/service/' + service) as response: ET.fromstring(response.read())
    print('PASS device and service descriptions:', name, flush=True)
    for target in ['urn:schemas-upnp-org:device:MediaRenderer:1', 'ssdp:all']:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.bind((ip, 0)); sock.settimeout(4)
        sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_IF, socket.inet_aton(ip))
        packet = f'M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: "ssdp:discover"\r\nMX: 1\r\nST: {target}\r\n\r\n'
        sock.sendto(packet.encode(), ('239.255.255.250', 1900))
        found = False
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            try: data, _ = sock.recvfrom(8192)
            except socket.timeout: break
            if base.encode() in data and b'HTTP/1.1 200 OK' in data:
                found = True; break
        sock.close(); assert found, f'No SSDP response for {target}'
    print('PASS multicast SSDP discovery', flush=True)
    with request('/event/AVTransport', 'SUBSCRIBE', {'NT': 'upnp:event', 'CALLBACK': f'<{callback}>', 'TIMEOUT': 'Second-60'}) as response:
        sid = response.headers['SID']
    deadline = time.monotonic() + 4
    while not notifications and time.monotonic() < deadline: time.sleep(.1)
    assert any(n[0] == sid and n[1] == 0 for n in notifications), 'No initial GENA event'
    with request('/event/AVTransport', 'SUBSCRIBE', {'SID': sid, 'TIMEOUT': 'Second-120'}) as response:
        assert response.headers['SID'] == sid
    print('PASS event subscription and renewal', flush=True)
    action('ConnectionManager', 'GetProtocolInfo')
    action('ConnectionManager', 'GetCurrentConnectionInfo', ConnectionID=0)
    action('AVTransport', 'SetAVTransportURI', InstanceID=0, CurrentURI=media, CurrentURIMetaData='')
    action('AVTransport', 'Play', InstanceID=0, Speed=1)
    wait_state('PLAYING')
    deadline = time.monotonic() + 15
    while True:
        position = action('AVTransport', 'GetPositionInfo', InstanceID=0)
        if position['RelTime'] != '00:00:00' or time.monotonic() >= deadline: break
        time.sleep(.25)
    assert position['TrackURI'] == media
    assert position['RelTime'] != '00:00:00', position
    print('PASS video playback and advancing position:', position['RelTime'], flush=True)
    action('AVTransport', 'Pause', InstanceID=0); wait_state('PAUSED_PLAYBACK')
    action('RenderingControl', 'SetVolume', InstanceID=0, Channel='Master', DesiredVolume=35)
    assert action('RenderingControl', 'GetVolume', InstanceID=0, Channel='Master')['CurrentVolume'] == '35'
    action('RenderingControl', 'SetMute', InstanceID=0, Channel='Master', DesiredMute=1)
    assert action('RenderingControl', 'GetMute', InstanceID=0, Channel='Master')['CurrentMute'] == '1'
    action('RenderingControl', 'SetMute', InstanceID=0, Channel='Master', DesiredMute=0)
    action('AVTransport', 'Seek', InstanceID=0, Unit='REL_TIME', Target='00:00:05')
    time.sleep(.5)
    assert action('AVTransport', 'GetPositionInfo', InstanceID=0)['RelTime'] >= '00:00:04'
    action('AVTransport', 'Play', InstanceID=0, Speed=1); wait_state('PLAYING')
    print('PASS pause, resume, seek, volume, mute', flush=True)
    try:
        action('RenderingControl', 'SetVolume', InstanceID=0, Channel='Master', DesiredVolume=101)
        raise AssertionError('Invalid volume accepted')
    except urllib.error.HTTPError as error:
        assert error.code == 500
        assert b'<errorCode>402</errorCode>' in error.read()
    action('AVTransport', 'SetAVTransportURI', InstanceID=0, CurrentURI=media, CurrentURIMetaData='')
    action('AVTransport', 'Play', InstanceID=0, Speed=1); wait_state('PLAYING')
    action('AVTransport', 'Stop', InstanceID=0); wait_state('STOPPED')
    action('AVTransport', 'Play', InstanceID=0, Speed=1); wait_state('PLAYING')
    action('AVTransport', 'Stop', InstanceID=0); wait_state('STOPPED')
    assert any(n[1] > 0 for n in notifications if n[0] == sid), 'No state-change event'
    with request('/event/AVTransport', 'UNSUBSCRIBE', {'SID': sid}) as response: assert response.status == 200
    print('PASS replacement, stop, replay, faults, state events, unsubscribe', flush=True)
finally:
    if 'sid' in globals():
        try:
            with request('/event/AVTransport', 'UNSUBSCRIBE', {'SID': sid}): pass
        except (urllib.error.URLError, TimeoutError): pass
    server.shutdown(); server.server_close()
