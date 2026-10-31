"""Explicitly package adopted standard style previews; no jobs, prompts or secrets."""
from pathlib import Path
import argparse,subprocess,json,base64,hashlib
p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--data-dir');a=p.parse_args()
root=Path(__file__).resolve().parents[1];out=root/'public/style-previews';out.mkdir(parents=True,exist_ok=True)
def rpc(op,args={}):
 cmd=[str(Path(a.binary).resolve())]+(['--data-dir',a.data_dir] if a.data_dir else [])+['--rpc']
 return json.loads(subprocess.run(cmd,input=json.dumps({'op':op,'args':args}),text=True,capture_output=True,check=True).stdout)
manifest=[]
for s in rpc('style.list'):
 if not s['id'].startswith('00000000-0000-4000-8000-') or not s.get('preview_asset_id'):continue
 asset=rpc('asset.get',{'id':s['preview_asset_id']})
 if asset['provider'] not in ['gemini','openai']:continue
 data=base64.b64decode(rpc('asset.read',{'id':asset['id'],'thumbnail':True})['data_base64'],validate=True)
 assert data.startswith(b'\x89PNG\r\n\x1a\n')
 name=s['id']+'.png';(out/name).write_bytes(data)
 manifest.append({'style_id':s['id'],'style_version':s['preview_style_version'],'name':s['name'],'blocks':asset['snapshot']['style']['blocks'],'provider':asset['provider'],'file':'/style-previews/'+name,'sha256':hashlib.sha256(data).hexdigest()})
(root/'src/bundled-style-previews.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print(f'{len(manifest)} adopted previews packaged')
