import fs from 'node:fs';
import {parseAst} from 'rolldown/parseAst';
function walk(n,visit){if(!n||typeof n!=='object')return;visit(n);for(const v of Object.values(n)){if(Array.isArray(v))v.forEach(x=>walk(x,visit));else if(v&&typeof v==='object')walk(v,visit);}}
const ast=parseAst(fs.readFileSync('src/i18n.ts','utf8'),{lang:'ts'},'i18n.ts');
let german;
walk(ast,n=>{if(n.type==='VariableDeclarator'&&n.id?.name==='german'){let init=n.init;while(init.expression)init=init.expression;german=Object.fromEntries(init.properties.map(p=>[p.key.name,p.value.value]));}});
const en=JSON.parse(fs.readFileSync('src/en.json'));
const all={...Object.fromEntries(Object.entries(german).map(([k,v])=>[v,en[k]])),...JSON.parse(fs.readFileSync('src/en-literals.json')),...JSON.parse(fs.readFileSync('src/en-errors.json'))};
const errors=[];
for(const k of Object.keys(german))if(typeof en[k]!=='string'||!en[k])errors.push('Missing English label: '+k);
for(const k of Object.keys(en))if(!(k in german))errors.push('Unused English key: '+k);
for(const file of fs.readdirSync('src').filter(f=>/\.(tsx|ts)$/.test(f)&&f!=='i18n.ts'&&!f.endsWith('.d.ts'))){
const a=parseAst(fs.readFileSync('src/'+file,'utf8'),{lang:file.endsWith('tsx')?'tsx':'ts'},file);
walk(a,n=>{if(n.type==='CallExpression'&&['t','formatText'].includes(n.callee?.name)&&n.arguments[0]?.type==='Literal'){const source=n.arguments[0].value;if(!all[source])errors.push(file+': Missing translation for '+source);}});
}
if(errors.length){console.error(errors.join('\n'));process.exitCode=1;}else console.log(Object.keys(german).length+' UI keys complete; all explicit UI literals have English translations.');
