# Throwaway spike: UI on :8801, untrusted origin on :8802.
import threading, http.server, base64, sys
CSP = ("sandbox allow-scripts; default-src 'none'; script-src 'self' 'unsafe-inline'; "
       "style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; "
       "connect-src 'none'; frame-ancestors http://127.0.0.1:8801")
PNG = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
BUNDLE_HTML = b"""<!doctype html><body><img id=rel src="pic.png"><img id=abs src="/a/x/1/pic.png">
<img id=remote src="http://127.0.0.1:8801/ui.png"><img id=datauri src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==">
<script src="app.js"></script>
<script>
const r={};
function img(id){return new Promise(res=>{const i=document.getElementById(id);
 const d=()=>res(i.complete&&i.naturalWidth>0); if(i.complete)d(); i.onload=()=>res(true); i.onerror=()=>res(false);});}
(async()=>{
 r.relImg=await img('rel'); r.absImg=await img('abs'); r.remoteImg=await img('remote'); r.dataImg=await img('datauri');
 r.externalScript=window.__ext===true;
 try{r.fetchSelf=await fetch('pic.png').then(x=>x.ok)}catch(e){r.fetchSelf='blocked'}
 try{r.cookie=document.cookie}catch(e){r.cookie='throws:'+e.name}
 try{r.ls=typeof localStorage}catch(e){r.ls='throws:'+e.name}
 try{r.parentDoc=!!parent.document.body}catch(e){r.parentDoc='throws:'+e.name}
 r.origin=location.origin; r.docUrl=location.href;
 parent.postMessage(JSON.stringify(r),'*');
})();
</script>"""
class UI(http.server.BaseHTTPRequestHandler):
    def do_GET(s):
        if s.path=='/ui.png':
            s.send_response(200);s.send_header('Content-Type','image/png');s.end_headers();s.wfile.write(PNG);return
        s.send_response(200);s.send_header('Content-Type','text/html')
        s.send_header('Set-Cookie','ui=1; Path=/'); s.end_headers()
        s.wfile.write(b"""<!doctype html><pre id=out>waiting</pre>
<iframe sandbox="allow-scripts" src="http://127.0.0.1:8802/a/x/1/index.html"></iframe>
<script>addEventListener('message',e=>{document.getElementById('out').textContent=e.data});</script>""")
class UT(http.server.BaseHTTPRequestHandler):
    def do_GET(s):
        p=s.path
        body,ct={'/a/x/1/index.html':(BUNDLE_HTML,'text/html'),'/a/x/1/pic.png':(PNG,'image/png'),
          '/a/x/1/app.js':(b"window.__ext=true;",'text/javascript'),
          '/a/x/1/evil.svg':(b'<svg xmlns="http://www.w3.org/2000/svg"><script>document.title="PWNED";window.__pwn=1</script></svg>','image/svg+xml')}.get(p,(b'nf','text/plain'))
        s.send_response(200 if body!=b'nf' else 404);s.send_header('Content-Type',ct)
        s.send_header('X-Content-Type-Options','nosniff');s.send_header('Content-Security-Policy',CSP if ct in ('text/html','text/javascript') else "sandbox; default-src 'none'; style-src 'unsafe-inline'");s.end_headers();s.wfile.write(body)
    def log_message(s,*a):pass
def run(h,p):http.server.ThreadingHTTPServer(('127.0.0.1',p),h).serve_forever()
threading.Thread(target=run,args=(UT,8802),daemon=True).start()
run(UI,8801)
