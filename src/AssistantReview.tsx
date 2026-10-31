import { t } from "./i18n";
import {useState} from 'react';
import {Field,Modal} from './components';
import {de} from './i18n';
import type {Draft,RecordData} from './types';
export default function AssistantReview({job,projectId,draft,onApply,onClose}:{job:RecordData;projectId:string;draft:Draft;onApply:(draft:Draft)=>void;onClose:()=>void}) {
 const target=job.snapshot?.parameters?.target||'instruction';
 const [text,setText]=useState(job.result_text||'');
 const [parts,setParts]=useState<Record<string,string>>(job.result_sections||{});
 const labels:Record<string,string>={scene:de.promptSceneText,character:de.promptCharacterNotes,instruction:de.promptInstruction};
 const structured=target==='all';
 const invalid=structured ? Object.keys(labels).some(k=>typeof parts[k]!=='string'||new TextEncoder().encode(parts[k]).length>8000)||!parts.instruction?.trim() : !text.trim()||new TextEncoder().encode(text).length>(target==='instruction'?12000:8000);
 function apply(){onApply(structured?{...draft,description:parts.instruction,blocks:{...draft.blocks,scene:parts.scene,character:parts.character}}:target==='scene'||target==='character'?{...draft,blocks:{...draft.blocks,[target]:text}}:{...draft,description:text});}
 return <Modal title={de.assistantTitle} onClose={onClose} wide>{structured?Object.entries(labels).map(([key,label])=><Field label={label} key={key}><textarea rows={4} value={parts[key]||''} onChange={e=>setParts({...parts,[key]:e.target.value})}/></Field>):<Field label={de.assistantResultText}><textarea rows={10} value={text} onChange={e=>setText(e.target.value)}/></Field>}<p className="hint">{de.promptAIOnlyTarget}</p><div className="form-actions"><button className="button" onClick={onClose}>{de.cancel}</button><button className="button primary" disabled={job.project_id!==projectId||invalid} onClick={apply}>{structured?t("Drei Bereiche übernehmen"):de.assistantApply}</button></div></Modal>;
}
