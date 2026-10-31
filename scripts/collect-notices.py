"""Collect installed package license/notice files for redistribution (no network)."""
from pathlib import Path
import json, tomllib
root = Path(__file__).resolve().parents[1]
cargo = tomllib.loads((root/'src-tauri/Cargo.lock').read_text())
npm = json.loads((root/'package-lock.json').read_text())
registry = Path.home()/'.cargo/registry/src'
parts = ['FrameStudio third-party notices\n\nFrameStudio source is MIT licensed. The following packages retain their own licenses.\nThis inventory covers installed locked packages, including build dependencies.\nNoto Sans: see OFL.txt. The SBOM identifies additional platform-specific locked packages.\n']
def collect(name, path):
    files = sorted(p for p in path.glob('*') if p.is_file() and p.name.lower().startswith(('license','licence','notice','copying','copyright')) and p.suffix.lower() not in ['.rs','.js','.ts','.py'])
    if files:
        parts.append('\n'+'='*72+'\n'+name+'\n')
        for p in files:
            parts.append('\n'+p.name+'\n'+p.read_text(errors='replace')+'\n')
for pkg in cargo['package']:
    for p in registry.glob('*/'+pkg['name']+'-'+pkg['version']):
        collect('Cargo: '+pkg['name']+' '+pkg['version'],p)
for path,pkg in npm['packages'].items():
    if path and pkg.get('version'):
        collect('npm: '+path.rsplit('node_modules/',1)[-1]+' '+pkg['version'],root/path)
out=root/'src-tauri/resources/THIRD_PARTY_NOTICES.txt'
out.write_text(''.join(parts))
print('License and notice inventory:',out.stat().st_size,'bytes')
