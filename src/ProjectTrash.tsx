import { t } from "./i18n";
import {useEffect,useState} from "react";
import {rpc,message} from "./api";
import {Modal,ErrorBox} from "./components";
import {de} from "./i18n";
import type {Project} from "./types";
export default function ProjectTrash({project,onClose,onSaved}:{project?:Project;onClose:()=>void;onSaved:()=>Promise<void>}) {
 const [items,setItems]=useState<Project[]>([]),[error,setError]=useState(""),[busy,setBusy]=useState(false);
 const load=()=>rpc<Project[]>("project.trash.list").then(setItems).catch(e=>setError(message(e)));
 useEffect(()=>{if(!project) void load();},[]);
 async function act(op:string,id:string){if(busy)return;setBusy(true);setError("");try{await rpc(op,{id});await onSaved();if(project)onClose();else await load();}catch(e){setError(message(e));}finally{setBusy(false);}}
 return <Modal title={project ? de.projectDelete : de.projectTrash} onClose={onClose}><p>{de.projectTrashHint}</p>{project ? <><h3>{project.name}</h3><ErrorBox error={error}/><div className="form-actions"><button className="button" onClick={onClose}>{de.cancel}</button><button className="button danger" disabled={busy} onClick={()=>void act("project.trash",project.id)}>{busy ? de.busy : t("In den Papierkorb verschieben")}</button></div></> : <><ErrorBox error={error}/>{items.length ? items.map(p=><div className="trash-row" key={p.id}><strong>{p.name}</strong><button className="button" disabled={busy} onClick={()=>void act("project.untrash",p.id)}>{de.projectRestoreTrash}</button></div>) : <p>{t("Der Papierkorb ist leer.")}</p>}</>}</Modal>;
}
