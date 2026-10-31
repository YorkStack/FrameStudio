"""Offline CycloneDX 1.6 dependency inventory, derived from reproducible locks."""
from pathlib import Path
import json, tomllib, hashlib
from urllib.parse import quote
root = Path(__file__).resolve().parents[1]
npm = json.loads((root/'package-lock.json').read_text(encoding="utf-8"))
cargo = tomllib.loads((root/'src-tauri/Cargo.lock').read_text(encoding="utf-8"))
components = {}
for path,p in npm['packages'].items():
    if not path or not p.get('version'): continue
    name = p.get('name') or path.rsplit('node_modules/',1)[-1]
    ref = 'pkg:npm/'+quote(name,safe='/')+'@'+p['version']
    c = {'type':'library','bom-ref':ref,'name':name,'version':p['version'],'purl':ref,'properties':[{'name':'framestudio:dependency-scope','value':'development' if p.get('dev') else 'runtime'}]}
    if p.get('license'): c['licenses']=[{'license':{'name':p['license']}}]
    components[ref] = c
for p in cargo['package']:
    ref = f"pkg:cargo/{p['name']}@{p['version']}"
    c = {'type':'library','bom-ref':ref,'name':p['name'],'version':p['version'],'purl':ref}
    if p.get('checksum'): c['hashes']=[{'alg':'SHA-256','content':p['checksum']}]
    components[ref] = c
version=json.loads((root/'package.json').read_text(encoding="utf-8"))['version']
assert version==tomllib.loads((root/'src-tauri/Cargo.toml').read_text(encoding="utf-8"))['package']['version']==json.loads((root/'src-tauri/tauri.conf.json').read_text(encoding="utf-8"))['version']
bom={'bomFormat':'CycloneDX','specVersion':'1.6','version':1,'metadata':{'component':{'type':'application','name':'FrameStudio','version':version},'lifecycles':[{'phase':'pre-build'}],'properties':[{'name':'framestudio:scope','value':'All locked npm and Cargo packages, including development and cross-platform dependencies. Not a runtime binary scan; OS frameworks and external APIs excluded.'}]+[{'name':'framestudio:sha256:'+p,'value':hashlib.sha256((root/p).read_bytes()).hexdigest()} for p in ['package-lock.json','src-tauri/Cargo.lock']]},'components':sorted(components.values(),key=lambda c:c['bom-ref'])}
out=root/'src-tauri/resources/sbom.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(bom,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(f'SBOM: {len(components)} locked components -> {out}')
