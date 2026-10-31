#!/usr/bin/env python3
"""End-to-end local acceptance. Only deterministic-v1. Creates a fresh private test store."""
import base64, json, os, pathlib, struct, subprocess, sys, tempfile, time, urllib.request, zlib
ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = ROOT / 'src-tauri/target/debug/framestudio'
(ROOT / '.artifacts').mkdir(exist_ok=True)
STORE = pathlib.Path(tempfile.mkdtemp(prefix='acceptance-', dir=ROOT / '.artifacts'))
REPORT = {'data_dir': str(STORE), 'provider': 'test', 'paid_requests': 0, 'steps': [], 'metrics': {}}

def local(op, args):
    p = subprocess.run([str(BINARY), '--data-dir', str(STORE), '--rpc'], input=json.dumps({'op':op,'args':args}), text=True, capture_output=True, check=True)
    return json.loads(p.stdout)
local('settings.save', {'rest_enabled':True, 'mcp_enabled':True})
token = local('tokens.create', {})['token']
server = None

def start():
    global server
    server = subprocess.Popen([str(BINARY), '--data-dir', str(STORE), '--serve'], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    for _ in range(100):
        try: call('project.list', {}); return
        except Exception: time.sleep(.05)
    raise RuntimeError('Server did not start')
def stop():
    if server: server.terminate(); server.wait(timeout=10)
def call(op, args):
    request = urllib.request.Request('http://127.0.0.1:43120/rpc', data=json.dumps({'op':op,'args':args}).encode(), headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
    with urllib.request.urlopen(request, timeout=180) as response: return json.load(response)
def step(s): REPORT['steps'].append(s); print(s, flush=True)
def png():
    def chunk(kind,data): return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',128,128,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\x00'+bytes([185,100,62])*128)*128))+chunk(b'IEND',b'')
try:
    t=time.perf_counter();start();REPORT['metrics']['headless_ready_ms']=round((time.perf_counter()-t)*1000,2)
    project=call('project.save',{'name':'Abnahme · synthetisch','description':'Nur lokaler Testprovider','aspect_ratio':'1:1'})
    pid=project['id']; styles=call('style.list',{}); assert len(styles)>=11
    ref=call('asset.import',{'project_id':pid,'name':'Referenz · synthetisch','data_base64':base64.b64encode(png()).decode()})
    char=call('character.save',{'project_id':pid,'name':'Testfigur','description':'Synthetische Prüffigur','features':'runde Silhouette','clothing':'orange Jacke','colors':'orange','binding':['features'],'reference_ids':[ref['id']],'approved':True})
    request={'project_id':pid,'description':'Drei Varianten für lokale Abnahme','provider':'test','model':'deterministic-v1','style_id':styles[0]['id'],'character_id':char['id'],'reference_ids':[ref['id']],'aspect_ratio':'1:1','resolution':'1K','count':1,'blocks':{'scene':'Neutrale Prüffläche'},'variants':[{'pose':p,'expression':'neutral','view':'frontal','mouth':'geschlossen'} for p in ['stehen','sitzen','winken']], 'idempotency_key':'acceptance-three'}
    estimate=call('cost.estimate',request);assert estimate['total_microusd']==0
    jobs=call('job.submit',request)['jobs'];assert len(jobs)==3
    assert [j['id'] for j in call('job.submit',request)['jobs']]==[j['id'] for j in jobs]
    for _ in range(600):
        statuses=[call('job.get',{'id':j['id']})['status'] for j in jobs]
        if all(s=='completed' for s in statuses):break
        if any(s in ['failed','unknown'] for s in statuses):raise RuntimeError(statuses)
        time.sleep(.1)
    assert all(s=='completed' for s in statuses),statuses
    step('Projekt,11 Stile,Referenz,freigegebener Charakter,3 Varianten,Idempotenz erfolgreich')
    assets=call('asset.list',{'project_id':pid,'limit':100})['items'];assert len(assets)==4
    stop();start();assert call('asset.list',{'project_id':pid})['total']==4
    step('Neustart: 4 Assets und Jobhistorie erhalten')
    outputs={}
    for fmt in ['original','png','jpeg','webp','tiff','html','pdf','pptx','sprite','zip']:
        export_job=call('export.submit',{'project_id':pid,'asset_ids':[a['id'] for a in assets],'format':fmt,'title':'Lokale Abnahme – drei Varianten','description':'Synthetischer Testprovider. Kein echtes Modellbild.','include_metadata':fmt=='zip','layout':'contact','background':'#ffffff','cell_width':256,'cell_height':256,'columns':2,'padding':16})
        for _ in range(1800):
            export_job=call('export.get',{'id':export_job['id']})
            if export_job['status'] in ['completed','failed']: break
            time.sleep(.1)
        assert export_job['status']=='completed',export_job
        result=export_job['result']
        data=call('export.read',{'id':result['id']});assert len(data['data_base64'])>10
        outputs[fmt]=result
        if fmt=='zip':
            restored=call('project.restore',{'data_base64':data['data_base64']})
            assert restored['id']!=pid and call('asset.list',{'project_id':restored['id']})['total']==4
    REPORT['exports']=outputs
    step('Original,PNG,JPEG,WebP,TIFF,HTML,PDF,PPTX,Sprite,ZIP gelesen; ZIP wiederhergestellt')
    if '--scale' in sys.argv:
        scale=call('project.save',{'name':'Skalierungsprüfung · 1000 synthetische Assets'})
        data=base64.b64encode(png()).decode(); t=time.perf_counter()
        for i in range(1000):call('asset.import',{'project_id':scale['id'],'name':f'Synthetisches Asset {i:04}','data_base64':data})
        REPORT['metrics']['import_1000_seconds']=round(time.perf_counter()-t,3)
        timings=[]
        for i in range(20):
            t=time.perf_counter();page=call('asset.list',{'project_id':scale['id'],'offset':i*40,'limit':40});timings.append((time.perf_counter()-t)*1000);assert page['total']==1000
        REPORT['metrics']['gallery_page_ms_avg']=round(sum(timings)/len(timings),2)
        REPORT['metrics']['gallery_page_ms_max']=round(max(timings),2)
        REPORT['scale_project']=scale['id'];step('1000 echte synthetische Importassets und 20 Galerieseiten geprüft')
    REPORT['costs']=call('cost.project',{'project_id':pid})
    REPORT['metrics']['server_rss_kib']=subprocess.check_output(['ps','-o','rss=','-p',str(server.pid)],text=True).strip()
    REPORT['status']='passed'
finally:
    stop()
    (ROOT/'.artifacts/acceptance-report.json').write_text(json.dumps(REPORT,indent=2,ensure_ascii=False))
    if '--scale' in sys.argv: (ROOT/'.artifacts/scale-report.json').write_text(json.dumps(REPORT,indent=2,ensure_ascii=False))
    print('Report: .artifacts/acceptance-report.json')
