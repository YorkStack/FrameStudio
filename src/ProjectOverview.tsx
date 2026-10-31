import { useState } from "react";
import { Archive, ArrowUpRight, FolderOpen, Plus, Search } from "lucide-react";
import { de } from "./i18n";
import type { Project, RecordData } from "./types";

export default function ProjectOverview({ projects, styles, archived, onArchiveView, onOpen, onNew, onDelete, onTrash }: {
  projects: Project[];
  styles: RecordData[];
  archived: boolean;
  onArchiveView: (value: boolean) => void;
  onOpen: (project: Project) => void;
  onNew: () => void;
  onDelete: (project:Project)=>void;
  onTrash: ()=>void;
}) {
  const [query, setQuery] = useState("");
  const inView = projects.filter((project) => Boolean(project.archived) === archived);
  const term = query.trim().toLocaleLowerCase("de");
  const visible = inView.filter((project) => `${project.name} ${project.description || ""}`.toLocaleLowerCase("de").includes(term));
  const switchView = () => { setQuery(""); onArchiveView(!archived); };
  return <section className="library project-overview" aria-label={de.projectOverview}>
    <div className="library-heading">
      <span className="eyebrow">{de.projectOverviewEyebrow}</span>
      <h2>{archived ? de.projectArchive : de.projects} <span className="project-overview-count">{inView.length}</span></h2>
      <p>{archived ? de.archiveNote : de.projectOverviewBody}</p>
      <button className="button primary" onClick={onNew}><Plus size={16} />{de.newProject}</button>
    </div>
    <div className="project-overview-toolbar">
      <label className="project-search"><Search size={17} /><input type="search" aria-label={de.projectSearch} placeholder={de.projectSearch} value={query} onChange={(event) => setQuery(event.target.value)} /></label>
      <button className="button" onClick={onTrash}>{de.projectTrash}</button>
      <button className="button" onClick={switchView}><Archive size={16} />{archived ? de.archiveHide : de.archiveShow}</button>
    </div>
    {visible.length ? <div className="project-overview-grid">
      {visible.map((project) => <article key={project.id} className="project-card-shell"><button className="project-card" aria-label={`${de.projectOpen}: ${project.name}`} onClick={() => onOpen(project)}>
        <span className="project-card-icon"><FolderOpen size={26} strokeWidth={1.5} /><ArrowUpRight size={19} /></span>
        <span className="project-card-name">{project.name}</span>
        <span className="project-card-description">{project.description || de.projectNoDescription}</span>
        <span className="project-card-meta">{styles.find((style) => style.id === project.style_id)?.name || de.projectNoStyle} · {project.aspect_ratio || "1:1"}</span>
        <span className="project-card-open">{de.projectOpen}<ArrowUpRight size={15} /></span>
      </button><button className="text-button project-card-delete" aria-label={`${de.projectDelete}: ${project.name}`} onClick={()=>onDelete(project)}>{de.projectDelete}</button></article>)}
    </div> : <div className="project-overview-empty"><FolderOpen size={34} strokeWidth={1} /><h3>{term ? de.projectSearchEmpty : archived ? de.projectArchiveEmpty : de.projectListEmpty}</h3><p>{term ? de.projectSearchHelp : archived ? de.archiveNote : de.noProjectBody}</p></div>}
  </section>;
}
