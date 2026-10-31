import { styleName } from "./i18n";
import { translateError } from "./i18n";
import { locale } from "./i18n";
import { t } from "./i18n";
import AssistantReview from "./AssistantReview";
import GenerationProgress from "./GenerationProgress";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Aperture,
  ArrowDownToLine,
  Archive,
  ArrowLeft,
  ArrowRight,
  Check,
  ChevronRight,
  Columns2,
  Copy,
  FileArchive,
  Folder,
  FolderOpen,
  Image,
  Info,
  Layers3,
  MoreHorizontal,
  Palette,
  Plus,
  Search,
  Settings2,
  SlidersHorizontal,
  Star,
  Upload,
  UserRound,
  X,
  ZoomIn,
  ZoomOut,
  RefreshCw,
  CircleCheck,
  Clock3,
} from "lucide-react";
import { rpc, message, money, fileBase64, isDesktop } from "./api";
import { de, statuses, costStatuses, setLanguage } from "./i18n";
import {
  AssetImage,
  Empty,
  ErrorBox,
  Field,
  FormActions,
  Modal,
} from "./components";
import {
  ProjectEditor,
  StyleEditor,
  CharacterEditor,
  SettingsEditor,
  ExportEditor,
} from "./editors";
import bundledPreviews from "./bundled-style-previews.json";
import ProjectTrash from "./ProjectTrash";
import ProjectOverview from "./ProjectOverview";
import SeriesApprovals from "./SeriesApprovals";
import Generator, { freshDraft } from "./Generator";
import StylePreview, { type PreparedStylePreview } from "./StylePreview";
import type { AppState, Asset, Draft, Job, Project, RecordData } from "./types";
const PAGE_SIZE = 72;
const running = (j: Job) =>
  ["queued", "running", "cancelling"].includes(j.status);
const initial: AppState = {
  projects: [],
  styles: [],
  characters: [],
  models: [],
  settings: {},
  key_status: false,
  data_dir: "",
};
export default function App() {
  const [state, setState] = useState<AppState>(initial);
  const [loaded, setLoaded] = useState(false);
  const [projectId, setProjectId] = useState("");
  const [assets, setAssets] = useState<Asset[]>([]);
  const [total, setTotal] = useState(0);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [costs, setCosts] = useState<RecordData>({});
  const [currentId, setCurrentId] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [query, setQuery] = useState("");
  const [favoriteOnly, setFavoriteOnly] = useState(false);
  const [characterFilter, setCharacterFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState("");
  const [dateFilter, setDateFilter] = useState("");
  const [offset, setOffset] = useState(0);
  const [section, setSection] = useState<"projects" | "styles" | "characters">(
    "projects",
  );
  const [quickMode, setQuickMode] = useState(false);
  const quickPending = useRef(false);
  const [projectOverview, setProjectOverview] = useState(false);
  const showingProjectOverview = section === "projects" && projectOverview;
  const [archived, setArchived] = useState(false);
  const [modal, setModal] = useState<string>("");
  const [editing, setEditing] = useState<RecordData | undefined>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [draft, setDraft] = useState<Draft>(freshDraft());
  const [zoom, setZoom] = useState(1);
  const [compare, setCompare] = useState(false);
  const [jobsOpen, setJobsOpen] = useState(false);
  const [focusedSeries, setFocusedSeries] = useState<RecordData | null>(null);
  const jobsHeading = useRef<HTMLHeadingElement>(null);
  const activeProjectId = useRef(projectId);
  activeProjectId.current = projectId;
  const jobsRequest = useRef(0);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [loadingAssets, setLoadingAssets] = useState(false);
  const [importing, setImporting] = useState(false);
  const [previewBusyId, setPreviewBusyId] = useState("");
  const importRef = useRef<HTMLInputElement>(null);
  const restoreRef = useRef<HTMLInputElement>(null);
  const jobSignature = useRef("");
  const assetRequest = useRef(0);
  const project = state.projects.find((p) => p.id === projectId);
  const current = assets.find((a) => a.id === currentId);
  const activeJobs = jobs.filter(running);
  const visibleJobs = focusedSeries?.project_id === projectId
    ? jobs.filter(job => job.series_id === focusedSeries.id) : jobs;
  const projectCharacters = state.characters.filter(
    (c) => c.project_id === projectId,
  );
  const refreshState = useCallback(async () => {
    const result = await rpc<AppState>("state");
    setLanguage(result.settings.language);
    setState(result);
    setProjectId((id) =>
      result.projects.some((p) => p.id === id)
        ? id
        : result.projects.find((p) => !p.archived)?.id || "",
    );
    setLoaded(true);
  }, []);
  const refreshAssets = useCallback(async (background = false) => {
    const request = ++assetRequest.current;
    if (!projectId) {
      setAssets([]);
      setTotal(0);
      return;
    }
    if (!background) setLoadingAssets(true);
    try {
      const result = await rpc("asset.list", {
        project_id: projectId,
        query: query || undefined,
        favorite: favoriteOnly || undefined,
        character_id: characterFilter || undefined,
        status: statusFilter || undefined,
        date: dateFilter || undefined,
        offset,
        limit: PAGE_SIZE,
      });
      if (request !== assetRequest.current || activeProjectId.current !== projectId) return;
      setAssets(result.items);
      setTotal(result.total);
      setCurrentId((id) =>
        result.items.some((a: Asset) => a.id === id)
          ? id
          : result.items[0]?.id || "",
      );
    } finally {
      if (request === assetRequest.current) setLoadingAssets(false);
    }
  }, [
    projectId,
    query,
    favoriteOnly,
    characterFilter,
    statusFilter,
    dateFilter,
    offset,
  ]);
  const refreshJobs = useCallback(async () => {
    const request = ++jobsRequest.current;
    if (!projectId) {
      setJobs([]);
      return;
    }
    const [j, c] = await Promise.all([
      rpc<Job[]>("job.list", { project_id: projectId }),
      rpc("cost.project", { project_id: projectId }),
    ]);
    if (request !== jobsRequest.current || activeProjectId.current !== projectId) return;
    setJobs(j);
    setCosts(c);
    const sig = j.map((job) => `${job.id}:${job.status}`).join("|");
    if (jobSignature.current !== sig) {
      jobSignature.current = sig;
      await refreshAssets();
    }
  }, [projectId, refreshAssets]);
  const run = useCallback(async (work: () => Promise<void>) => {
    setError("");
    try {
      await work();
    } catch (e) {
      setError(message(e));
    }
  }, []);
  useEffect(() => {
    void run(refreshState);
  }, [refreshState, run]);
  useEffect(() => {
    const timer = setTimeout(() => void run(refreshAssets), 180);
    return () => clearTimeout(timer);
  }, [refreshAssets, run]);
  // REST/MCP imports do not change any job status. Refresh the bounded gallery
  // page independently, without loading originals or flashing a spinner.
  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") void refreshAssets(true).catch(e => setError(message(e)));
    }, 5000);
    return () => clearInterval(timer);
  }, [refreshAssets]);
  useEffect(() => {
    void run(refreshJobs);
    const timer = setInterval(
      () => void refreshJobs().catch((e) => setError(message(e))),
      2000,
    );
    return () => clearInterval(timer);
  }, [refreshJobs, run]);
  useEffect(() => {
    if(section !== "styles" && section !== "characters") return;
    const timer=setInterval(()=>{void refreshState().catch(e=>setError(message(e)));},2000);
    return ()=>clearInterval(timer);
  },[section,refreshState]);
  useEffect(() => {
    const p = state.projects.find((p) => p.id === projectId);
    setDraft(freshDraft(p));
    setSelected([]);
    setOffset(0);
    setQuery("");
    setCompare(false);
    setZoom(1);
    setCharacterFilter("");
    setStatusFilter("");
    setDateFilter("");
  }, [projectId]);
  useEffect(() => {
    document.documentElement.dataset.theme = state.settings.theme || "system";
  }, [state.settings.theme]);
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(""), 7000);
    return () => clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    const handle = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        setEditing(undefined);
        setModal("project");
        return;
      }
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement ||
        modal || showingProjectOverview || section !== "projects"
      )
        return;
      const index = assets.findIndex((a) => a.id === currentId);
      if (e.key === "ArrowRight" && assets[index + 1]) {
        e.preventDefault();
        setCurrentId(assets[index + 1].id);
      }
      if (e.key === "ArrowLeft" && assets[index - 1]) {
        e.preventDefault();
        setCurrentId(assets[index - 1].id);
      }
      if (e.key === "Escape") {
        setCompare(false);
        setSelected([]);
      }
    };
    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, [assets, currentId, modal, showingProjectOverview, section]);
  async function afterSubmit() {
    await refreshJobs();
    setJobsOpen(true);
  }
  function chooseProject(p: Project) {
    setQuickMode(false);
    if (p.id !== activeProjectId.current) {
      ++assetRequest.current;
      ++jobsRequest.current;
      setAssets([]); setTotal(0); setCurrentId("");
      setJobs([]); setCosts({}); jobSignature.current = "";
    }
    activeProjectId.current = p.id;
    setFocusedSeries(null);
    setProjectId(p.id);
    setSection("projects");
    setProjectOverview(false);
    setArchived(Boolean(p.archived));
    setJobsOpen(false);
    setMenuOpen(false);
  }
  async function openSeriesProject(series: RecordData) {
    // REST/MCP may have created this project since the last state refresh.
    const [target, targetJobs, targetCosts] = await Promise.all([
      rpc<Project>("project.get", { id: series.project_id }),
      rpc<Job[]>("job.list", { project_id: series.project_id }),
      rpc("cost.project", { project_id: series.project_id }),
    ]);
    setState(current => ({ ...current, projects: [target, ...current.projects.filter(p => p.id !== target.id)] }));
    chooseProject(target);
    ++jobsRequest.current;
    setJobs(targetJobs);
    setCosts(targetCosts);
    setFocusedSeries(series);
    setJobsOpen(true);
  }
  useEffect(() => {
    if (jobsOpen && focusedSeries) {
      const frame = requestAnimationFrame(() => jobsHeading.current?.focus());
      return () => cancelAnimationFrame(frame);
    }
  }, [jobsOpen, focusedSeries]);
  function showProjects(showArchived = false) {
    setQuickMode(false);
    setSection("projects");
    setProjectOverview(true);
    setArchived(showArchived);
    setJobsOpen(false);
    setMenuOpen(false);
  }
  async function openSingleImage() {
    if (quickPending.current) return;
    quickPending.current = true;
    try {
      const project = state.projects.find(p=>p.preferences?.purpose === "single_images" && !p.archived)
        || await rpc<Project>("project.save", {name:de.singleImages, description:"", preferences:{purpose:"single_images"}});
      await refreshState();
      chooseProject(project);
      setDraft(freshDraft(project));
      setQuickMode(true);
    } catch(e) { setError(message(e)); }
    finally { quickPending.current = false; }
  }
  function toggleSelected(id: string) {
    setSelected((ids) =>
      ids.includes(id) ? ids.filter((v) => v !== id) : [...ids, id],
    );
  }
  async function importFiles(files: FileList | null) {
    if (!files || !projectId) return;
    setImporting(true);
    await run(async () => {
      for (const file of Array.from(files)) {
        const a = await rpc("asset.import", {
          project_id: projectId,
          name: file.name,
          description: file.name.replace(/\.[^.]+$/, ""),
          data_base64: await fileBase64(file),
        });
        setCurrentId(a.id);
      }
      await refreshAssets();
      setNotice(t("Referenzen importiert. Originaldateien bleiben unverändert."));
    });
    setImporting(false);
    if (importRef.current) importRef.current.value = "";
  }
  async function restoreFile(file?: File) {
    if (!file) return;
    await run(async () => {
      const p = await rpc("project.restore", {
        data_base64: await fileBase64(file),
      });
      await refreshState();
      setProjectId(p.id || p.project?.id);
      setNotice(t("Projekt wiederhergestellt."));
    });
    if (restoreRef.current) restoreRef.current.value = "";
  }
  function useAsset(a: Asset, variant = false) {
    setDraft((d) => {
      if (!variant)
        return {
          ...d,
          reference_ids: Array.from(new Set([...d.reference_ids, a.id])),
        };
      const p = a.snapshot?.parameters || a.parameters || {};
      return {
        ...freshDraft(project),
        description: a.description || d.description,
        style_id: p.style_id || d.style_id,
        character_id: p.character_id || "",
        provider: p.provider || d.provider,
        model: p.model || d.model,
        aspect_ratio: p.aspect_ratio || d.aspect_ratio,
        resolution: p.resolution || d.resolution,
        quality: p.quality,
        blocks: p.blocks || d.blocks,
        reference_ids: Array.from(new Set([...(a.reference_ids || []), a.id])),
        parent_id: a.id,
      };
    });
    setNotice(
      variant
        ? t("Bild als Ausgangspunkt einer neuen Variante übernommen.")
        : t("Referenz zum nächsten Auftrag hinzugefügt."),
    );
  }
  async function favorite(a: Asset) {
    await run(async () => {
      await rpc("asset.update", { id: a.id, favorite: !a.favorite });
      await refreshAssets();
    });
  }
  async function saveMode(mode: string) {
    await run(async () => {
      const settings = await rpc("settings.save", { ...state.settings, mode });
      setState((s) => ({ ...s, settings: settings.settings || settings }));
    });
  }
  async function prepareStylePreview(style: RecordData) {
    if (previewBusyId) return;
    setPreviewBusyId(style.id);
    await run(async () => {
      const prepared = await rpc<PreparedStylePreview>("style.preview.prepare", { id: style.id });
      await refreshState();
      setEditing(prepared);
      setModal("style-preview");
    });
    setPreviewBusyId("");
  }
  async function removeStylePreview(style: RecordData) {
    await run(async () => {
      await rpc("style.preview.set", { id: style.id, asset_id: null });
      await refreshState();
      setNotice(de.stylePreviewRemoved);
    });
  }
  const openEdit = (kind: string, item?: RecordData) => {
    setEditing(item);
    setModal(kind);
  };
  function openExport(format?: string, seriesId?: string) {
    setEditing({ format, seriesId });
    setModal("export");
    setMenuOpen(false);
  }
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <Aperture size={25} strokeWidth={1.7} />
          </div>
          <div>
            <h1>{de.app}</h1>
            <span>GRAPHIC WORKSHOP</span><small className="app-version">Version {state.app_version || "0.3.0"}</small>
          </div>
        </div>
        <button className="new-project" onClick={() => openEdit("project")}>
          <Plus size={17} />
          {de.newProject}
          <span aria-hidden="true">{/Mac/.test(navigator.platform) ? "⌘N" : "Ctrl+N"}</span>
        </button>
        <nav className="main-nav" aria-label={t("Bibliotheken")}>
          <button className={quickMode ? "active" : ""} onClick={()=>void openSingleImage()}><Image size={18}/>{de.singleImage}</button>
          <button
            className={section === "projects" && !quickMode ? "active" : ""}
            onClick={() => showProjects()}
          >
            <FolderOpen size={18} />
            {de.projects}
            <span>{state.projects.filter((p) => !p.archived).length}</span>
          </button>
          <button
            className={section === "styles" ? "active" : ""}
            onClick={() => {setQuickMode(false);setSection("styles");}}
          >
            <Palette size={18} />
            {de.styles}
            <span>{state.styles.length}</span>
          </button>
          <button
            className={section === "characters" ? "active" : ""}
            onClick={() => {setQuickMode(false);setSection("characters");}}
          >
            <UserRound size={18} />
            {de.characters}
            <span>{projectCharacters.length}</span>
          </button>
        </nav>
        <div className="sidebar-section-title">
          <span>{archived ? t("ARCHIV") : t("DEINE PROJEKTE")}</span>
          <button
            className="icon-button"
            aria-label={archived ? de.archiveHide : de.archiveShow}
            onClick={() => showProjects(!archived)}
          >
            <Archive size={14} />
          </button>
        </div>
        <div className="project-list">
          {state.projects
            .filter((p) => Boolean(p.archived) === archived)
            .map((p) => (
              <button
                className={`project-item ${p.id === projectId && section === "projects" && !projectOverview ? "active" : ""}`}
                key={p.id}
                onClick={() => chooseProject(p)}
              >
                <span className="project-glyph">
                  <Folder size={15} />
                </span>
                <span>{p.name}</span>
                {p.id === projectId && section === "projects" && !projectOverview && <span className="project-dot" />}
              </button>
            ))}
          {!state.projects.some((p) => Boolean(p.archived) === archived) && (
            <p className="sidebar-empty">
              {archived
                ? t("Das Archiv ist leer.")
                : t("Dein erster kreativer Raum wartet.")}
            </p>
          )}
        </div>
        <div className="sidebar-bottom">
          <div className="local-badge">
            <span className="status-dot" />
            <span>{t("Lokale Werkstatt")}</span>
            <ShieldIcon />
          </div>
          <p>{de.savedLocally}</p>
          <button
            className="sidebar-settings"
            disabled={!loaded}
            onClick={() => setModal("settings")}
          >
            <Settings2 size={18} />
            {de.settings}
            <ChevronRight size={15} />
          </button>
        </div>
      </aside>
      <main className="main-workspace">
        <header className="topbar">
          <div className="breadcrumb">
            <span>{t("Werkstatt")}</span>
            <ChevronRight size={13} />
            <strong>
              {section === "styles"
                ? de.styles
                : section === "characters"
                  ? de.characters
                  : showingProjectOverview ? (archived ? de.projectArchive : de.projects) : project?.name || t("Willkommen")}
            </strong>
            {project?.archived && !showingProjectOverview && <span className="badge">{t("Archiv")}</span>}
          </div>
          <div className="topbar-actions">
            <SeriesApprovals state={state} onOpenProject={openSeriesProject} onExportSeries={async series => { await openSeriesProject(series); setJobsOpen(false); openExport(undefined, series.id); }} />
            {!showingProjectOverview && <button
              className="status-button"
              aria-label={`${de.jobs}: ${activeJobs.length ? `${activeJobs.length} ${de.generationOpen}` : de.ready}`}
              aria-expanded={jobsOpen}
              onClick={() => setJobsOpen(!jobsOpen)}
            >
              <span
                className={`status-dot ${activeJobs.length ? "working" : ""}`}
              />
              {activeJobs.length ? `${activeJobs.length} ${de.generationOpen}` : de.ready}
            </button>}
            {project && !showingProjectOverview && (
              <>
                <button
                  className="button small"
                  onClick={() => importRef.current?.click()}
                  disabled={importing}
                >
                  <Upload size={15} />
                  {importing ? de.busy : de.import}
                </button>
                <button
                  className="button small dark-button"
                  onClick={() => openExport()}
                >
                  <ArrowDownToLine size={15} />
                  {de.export}
                </button>
              </>
            )}
            <div className="menu-anchor">
              <button
                className="icon-button"
                aria-label={t("Projektaktionen")}
                onClick={() => setMenuOpen(!menuOpen)}
              >
                <MoreHorizontal size={20} />
              </button>
              {menuOpen && (
                <div className="dropdown-menu">
                  {project && !showingProjectOverview && (
                    <>
                      <button
                        onClick={() => {
                          openEdit("project", project);
                          setMenuOpen(false);
                        }}
                      >
                        <SlidersHorizontal size={15} />
                        {de.projectDefaults}
                      </button>
                      <button
                        onClick={() =>
                          void run(async () => {
                            const p = await rpc("project.duplicate", {
                              id: project.id,
                            });
                            await refreshState();
                            setProjectId(p.id);
                            setMenuOpen(false);
                          })
                        }
                      >
                        <Copy size={15} />
                        {de.duplicate}
                      </button>
                      <button
                        onClick={() =>
                          void run(async () => {
                            await rpc("project.save", {
                              ...project,
                              archived: !project.archived,
                            });
                            await refreshState();
                            setMenuOpen(false);
                          })
                        }
                      >
                        <Archive size={15} />
                        {project.archived ? de.restore : de.archive}
                      </button>
                      <button onClick={() => openExport("zip")}>
                        <FileArchive size={15} />
                        {de.backup}
                      </button>
                    </>
                  )}
                  <button
                    onClick={() => {
                      restoreRef.current?.click();
                      setMenuOpen(false);
                    }}
                  >
                    <Upload size={15} />
                    {de.restoreBackup}
                  </button>
                  <button
                    onClick={() =>
                      void run(async () => {
                        await refreshState();
                        await refreshAssets();
                        setMenuOpen(false);
                      })
                    }
                  >
                    <RefreshCw size={15} />
                    {de.refresh}
                  </button>
                </div>
              )}
            </div>
          </div>
        </header>
        {error && (
          <div className="global-error" role="alert">
            <Info size={17} />
            <span>{translateError(error)}</span>
            <button
              onClick={() => setError("")}
              className="icon-button"
              aria-label={de.close}
            >
              <X size={16} />
            </button>
          </div>
        )}
        {notice && (
          <div className="toast" role="status">
            <CircleCheck size={17} />
            {notice}
            <button
              className="icon-button"
              aria-label={de.close}
              onClick={() => setNotice("")}
            >
              <X size={14} />
            </button>
          </div>
        )}
        {loaded && project && !showingProjectOverview && <GenerationProgress jobs={jobs} onExportSeries={id => openExport(undefined, id)} onDetails={() => { setFocusedSeries(null); setJobsOpen(true); }} />}
        {!loaded ? (
          <Empty
            icon={<Aperture size={40} />}
            title={error ? de.offline : de.loading}
          >
            <button className="button" onClick={() => void run(refreshState)}>
              {de.retry}
            </button>
          </Empty>
        ) : showingProjectOverview ? (
          <ProjectOverview key={archived ? "archive" : "active"} projects={state.projects} styles={state.styles} archived={archived} onArchiveView={showProjects} onOpen={chooseProject} onNew={() => openEdit("project")} onDelete={p=>{setEditing(p);setModal("project-delete");}} onTrash={()=>setModal("project-trash")} />
        ) : !project && section !== "styles" ? (
          <div className="welcome">
            <span className="eyebrow">{t("EIN ORT FÜR VISUELLE IDEEN")}</span>
            <h2>{de.noProject}</h2>
            <p>{de.noProjectBody}</p>
            <button
              className="button primary"
              onClick={() => openEdit("project")}
            >
              <Plus size={18} />
              {de.newProject}
            </button>
            <button
              className="text-button"
              onClick={() => restoreRef.current?.click()}
            >
              {de.restoreBackup}
            </button>
            <div className="welcome-note">
              <div>
                <Folder size={21} />
                <span>{t("Projekte, die bei dir bleiben.")}</span>
              </div>
              <div>
                <Palette size={21} />
                <span>{t("Stile, die wiedererkennbar sind.")}</span>
              </div>
              <div>
                <Layers3 size={21} />
                <span>{t("Varianten mit nachvollziehbarer Herkunft.")}</span>
              </div>
            </div>
            <small>{de.noPaid}</small>
          </div>
        ) : section !== "projects" ? (
          <Library
            section={section}
            state={state}
            projectId={projectId}
            onEdit={openEdit}
            onPreview={(item) => void prepareStylePreview(item)}
            onRemovePreview={(item) => void removeStylePreview(item)}
            previewBusyId={previewBusyId}
            onDuplicate={(id) =>
              void run(async () => {
                await rpc("style.duplicate", { id });
                await refreshState();
              })
            }
          />
        ) : (
          project && (
            <div className="workbench">
              <div className="canvas-column">
                <div className="canvas-heading">
                  <div>
                    <span className="eyebrow">{t("PROJEKT")}</span>
                    <h2>{project.name}</h2>
                    <p>
                      {project.description ||
                        t("Eine neue Idee nimmt Gestalt an.")}
                    </p>
                  </div>
                  <div className="cost-summary">
                    <span>{de.projectCosts}</span>
                    <strong>{money(costs.calculated_microusd)}</strong>
                    <small>
                      {costs.unknown_count
                        ? `${costs.unknown_count} × Kosten unbekannt`
                        : de.calculated}
                    </small>
                  </div>
                </div>
                <div className="canvas-toolbar">
                  <div>
                    <span className="canvas-label">
                      {compare
                        ? de.compare
                        : current
                          ? current.description || de.image
                          : t("Arbeitsfläche")}
                    </span>
                    {current && (
                      <span className="dimension-label">
                        {current.width} × {current.height}
                      </span>
                    )}
                  </div>
                  <div className="toolbar-actions">
                    <button
                      className={`icon-button ${compare ? "selected" : ""}`}
                      disabled={assets.length < 2}
                      aria-label={de.compare}
                      title={t("Zwei ausgewählte Bilder vergleichen")}
                      onClick={() => {
                        if (selected.length < 2)
                          setSelected(assets.slice(0, 2).map((a) => a.id));
                        setCompare(!compare);
                      }}
                    >
                      <Columns2 size={17} />
                    </button>
                    <span className="toolbar-separator" />
                    <button
                      className="icon-button"
                      aria-label={t("Verkleinern")}
                      disabled={!current || zoom <= 0.25}
                      onClick={() => setZoom((z) => Math.max(0.25, z - 0.25))}
                    >
                      <ZoomOut size={17} />
                    </button>
                    <button
                      className="zoom-label"
                      onClick={() => setZoom(1)}
                      title={de.fit}
                    >
                      {zoom === 1 ? de.fit : `${Math.round(zoom * 100)} %`}
                    </button>
                    <button
                      className="icon-button"
                      aria-label={t("Vergrößern")}
                      disabled={!current || zoom >= 3}
                      onClick={() => setZoom((z) => Math.min(3, z + 0.25))}
                    >
                      <ZoomIn size={17} />
                    </button>
                    {current && (
                      <button
                        className="icon-button"
                        aria-label={de.details}
                        onClick={() => openEdit("asset", current)}
                      >
                        <Info size={17} />
                      </button>
                    )}
                  </div>
                </div>
                <div className={`image-stage ${compare ? "comparison" : ""}`}>
                  {compare ? (
                    selected.slice(0, 2).map((id) => (
                      <div className="compare-pane" key={id}>
                        <AssetImage
                          id={id}
                          thumbnail={false}
                          alt={assets.find((a) => a.id === id)?.description}
                        />
                        <span>
                          {assets.find((a) => a.id === id)?.description ||
                            id.slice(0, 8)}
                        </span>
                      </div>
                    ))
                  ) : current ? (
                    <div
                      className="hero-image"
                      style={{ transform: `scale(${zoom})` }}
                    >
                      <AssetImage
                        id={current.id}
                        thumbnail={false}
                        alt={current.description}
                      />
                    </div>
                  ) : (
                    <div className="canvas-empty">
                      <div className="empty-frame">
                        <Aperture size={39} strokeWidth={1} />
                        <span>FS</span>
                      </div>
                      <h3>{de.noAssets}</h3>
                      <p>{de.noAssetsBody}</p>
                      <button
                        className="button"
                        onClick={() => importRef.current?.click()}
                      >
                        <Upload size={15} />
                        {de.import}
                      </button>
                    </div>
                  )}
                  {current && !compare && (
                    <div className="image-stage-actions">
                      <button
                        className={current.favorite ? "favorited" : ""}
                        aria-label={
                          current.favorite ? de.unfavorite : de.favorite
                        }
                        onClick={() => void favorite(current)}
                      >
                        <Star
                          size={17}
                          fill={current.favorite ? "currentColor" : "none"}
                        />
                      </button>
                      <button onClick={() => useAsset(current, true)}>
                        <Copy size={16} />
                        {de.variant}
                      </button>
                      <button onClick={() => useAsset(current)}>
                        <Plus size={16} />
                        {de.reference}
                      </button>
                    </div>
                  )}
                </div>
                <section className="gallery" aria-label={de.gallery}>
                  <div className="gallery-heading">
                    <div>
                      <h3>{de.gallery}</h3>
                      <span className="count">{total}</span>
                      {loadingAssets && <span className="loading-dot" />}
                    </div>
                    <div>
                      <button
                        className={`icon-button ${favoriteOnly ? "selected" : ""}`}
                        aria-label={de.favorites}
                        aria-pressed={favoriteOnly}
                        onClick={() => {
                          setFavoriteOnly(!favoriteOnly);
                          setOffset(0);
                        }}
                      >
                        <Star size={16} />
                      </button>
                      <button
                        className={`icon-button ${filtersOpen ? "selected" : ""}`}
                        aria-label={de.filters}
                        aria-expanded={filtersOpen}
                        onClick={() => setFiltersOpen(!filtersOpen)}
                      >
                        <SlidersHorizontal size={16} />
                      </button>
                      <label className="search">
                        <Search size={15} />
                        <input
                          aria-label={de.search}
                          placeholder={t("Bilder suchen …")}
                          value={query}
                          onChange={(e) => {
                            setQuery(e.target.value);
                            setOffset(0);
                          }}
                        />
                      </label>
                    </div>
                  </div>
                  {filtersOpen && (
                    <div className="gallery-filters">
                      <select
                        aria-label={de.character}
                        value={characterFilter}
                        onChange={(e) => {
                          setCharacterFilter(e.target.value);
                          setOffset(0);
                        }}
                      >
                        <option value="">{t("Alle Charaktere")}</option>
                        {projectCharacters.map((c) => (
                          <option key={c.id} value={c.id}>
                            {c.name}
                          </option>
                        ))}
                      </select>
                      <select
                        aria-label={de.status}
                        value={statusFilter}
                        onChange={(e) => {
                          setStatusFilter(e.target.value);
                          setOffset(0);
                        }}
                      >
                        <option value="">{t("Alle Status")}</option>
                        {[
                          "ready",
                          "reference",
                          "approved",
                          "rejected",
                          "draft",
                        ].map((s) => (
                          <option value={s} key={s}>
                            {statuses[s] || s}
                          </option>
                        ))}
                      </select>
                      <input
                        aria-label={de.date}
                        type="date"
                        value={dateFilter}
                        onChange={(e) => {
                          setDateFilter(e.target.value);
                          setOffset(0);
                        }}
                      />
                      <button
                        className="text-button"
                        onClick={() => {
                          setCharacterFilter("");
                          setStatusFilter("");
                          setDateFilter("");
                          setQuery("");
                          setFavoriteOnly(false);
                          setOffset(0);
                        }}
                      >
                        {de.clear}
                      </button>
                    </div>
                  )}
                  {selected.length > 0 && (
                    <div className="selection-bar">
                      <span>
                        <Check size={14} />
                        {selected.length} {de.selected}
                      </span>
                      <button onClick={() => openExport()}>
                        {de.exportSelection}
                      </button>
                      <button
                        onClick={() => setSelected(assets.map((a) => a.id))}
                      >
                        {de.selectAll}
                      </button>
                      <button
                        className="icon-button"
                        aria-label={de.clearSelection}
                        onClick={() => setSelected([])}
                      >
                        <X size={14} />
                      </button>
                    </div>
                  )}
                  <div className="gallery-grid">
                    {assets.map((a, index) => (
                      <article
                        className={`asset-card ${currentId === a.id ? "current" : ""} ${selected.includes(a.id) ? "checked" : ""}`}
                        key={a.id}
                      >
                        <button
                          className="asset-open"
                          onClick={() => {
                            setCurrentId(a.id);
                            setZoom(1);
                          }}
                          onDoubleClick={() => openEdit("asset", a)}
                          aria-label={`${de.image} ${index + 1}: ${a.description}`}
                        >
                          <AssetImage id={a.id} alt={a.description} />
                        </button>
                        <button
                          className="asset-checkbox"
                          aria-label={`${de.select}: ${a.description}`}
                          aria-pressed={selected.includes(a.id)}
                          onClick={() => toggleSelected(a.id)}
                        >
                          {selected.includes(a.id) && <Check size={12} />}
                        </button>
                        <button
                          className={`asset-star ${a.favorite ? "on" : ""}`}
                          aria-label={a.favorite ? de.unfavorite : de.favorite}
                          onClick={() => void favorite(a)}
                        >
                          <Star
                            size={13}
                            fill={a.favorite ? "currentColor" : "none"}
                          />
                        </button>
                        <div className="asset-caption">
                          <span>{a.description || de.image}</span>
                          <small>
                            {a.width} × {a.height}{" "}
                            <span>{String(a.format || "").toUpperCase()}</span>
                          </small>
                        </div>
                      </article>
                    ))}
                    {assets.length === 0 && (
                      <p className="gallery-empty">
                        {query ||
                        favoriteOnly ||
                        statusFilter ||
                        characterFilter ||
                        dateFilter
                          ? de.empty
                          : t("Hier erscheinen deine Bilder und Referenzen.")}
                      </p>
                    )}
                  </div>
                  {total > PAGE_SIZE && (
                    <div className="pagination">
                      <span>
                        {offset + 1}–{Math.min(offset + PAGE_SIZE, total)} /{" "}
                        {total}
                      </span>
                      <button
                        className="button small"
                        disabled={offset === 0}
                        onClick={() =>
                          setOffset(Math.max(0, offset - PAGE_SIZE))
                        }
                      >
                        <ArrowLeft size={14} />
                        {de.previous}
                      </button>
                      <button
                        className="button small"
                        disabled={offset + PAGE_SIZE >= total}
                        onClick={() => setOffset(offset + PAGE_SIZE)}
                      >
                        {de.next}
                        <ArrowRight size={14} />
                      </button>
                    </div>
                  )}
                </section>
                <footer className="workspace-status">
                  <span>
                    <span className="status-dot" />
                    {de.local}
                  </span>
                  <span>
                    {money(costs.reserved_microusd)} {de.reserved.toLowerCase()}{" "}
                    · {money(costs.estimated_microusd)}{" "}
                    {de.estimated.toLowerCase()}
                  </span>
                </footer>
              </div>
              <Generator
                state={state}
                quickMode={quickMode}
                project={project}
                assets={assets}
                draft={draft}
                setDraft={setDraft}
                onSubmitted={afterSubmit}
                onEditCharacter={character => { setEditing(character); setModal("character"); }}
                onEditProject={() => { setEditing(project); setModal("project"); }}
                onTemplateSaved={refreshState}
                onReferencesImported={refreshAssets}
                onMode={(mode) => void saveMode(mode)}
              />
            </div>
          )
        )}
        {jobsOpen && (
          <section className="jobs-drawer" aria-label={de.jobs}>
            <div className="panel-heading">
              <h2 ref={jobsHeading} tabIndex={-1}>{de.jobs} · {project?.name}</h2>
              <button
                className="icon-button"
                aria-label={de.close}
                onClick={() => setJobsOpen(false)}
              >
                <X size={18} />
              </button>
            </div>
            {focusedSeries?.project_id === projectId && <div className="series-job-context">
              <span className="eyebrow">{de.seriesJobContext}</span>
              <strong>{focusedSeries.description}</strong>
              <p className="hint" role="status">{Object.entries(visibleJobs.reduce<Record<string, number>>((counts, job) => { counts[job.status] = (counts[job.status] || 0) + 1; return counts; }, {})).map(([status, count]) => `${count} × ${statuses[status] || status}`).join(" · ")}</p>
              <button className="text-button" onClick={() => setFocusedSeries(null)}>{de.allProjectJobs}</button>
            </div>}
            <p className="hint">{de.requestUnclear}</p>
            {visibleJobs.length ? (
              visibleJobs.map((j) => (
                <div className={`job-row ${j.status}`} key={j.id}>
                  <div className="job-icon">
                    {running(j) ? <Clock3 size={18} /> : <Image size={18} />}
                  </div>
                  <div>
                    {j.kind === "prompt_assist" && <span className="job-assistant-label">{de.assistantTitle}</span>}
                    <strong>
                      {j.description ||
                        j.snapshot?.parameters?.description ||
                        `Auftrag ${j.id.slice(0, 8)}`}
                    </strong>
                    <span>
                      {statuses[j.status] || j.status} ·{" "}
                      {j.provider || j.snapshot?.parameters?.provider}
                    </span>
                    {j.error && <p className="job-error">{translateError(j.error)}</p>}
                    {j.kind === "prompt_assist" && typeof j.result_text === "string" && j.result_text && <button className="button small" onClick={() => { setEditing(j); setModal("assistant-result"); }}>{de.assistantView}</button>}
                    {j.cost_status && (
                      <small>
                        {costStatuses[j.cost_status] || j.cost_status} ·{" "}
                        {money(j.actual_microusd)} · {de.estimated}:{" "}
                        {money(j.estimated_microusd)}
                      </small>
                    )}
                  </div>
                  {running(j) && (
                    <button
                      className="button small"
                      onClick={() =>
                        void run(async () => {
                          await rpc("job.cancel", { id: j.id });
                          await refreshJobs();
                        })
                      }
                    >
                      {de.cancel}
                    </button>
                  )}
                </div>
              ))
            ) : (
              <p className="hint">{t("Noch keine Generierungsaufträge.")}</p>
            )}
          </section>
        )}
      </main>
      <input
        ref={importRef}
        type="file"
        hidden
        accept="image/png,image/jpeg,image/webp,image/tiff"
        multiple
        onChange={(e) => void importFiles(e.target.files)}
      />
      <input
        ref={restoreRef}
        type="file"
        hidden
        accept=".zip,application/zip"
        onChange={(e) => void restoreFile(e.target.files?.[0])}
      />
      {(modal === "project-delete" || modal === "project-trash") && <ProjectTrash project={modal === "project-delete" ? editing as Project : undefined} onClose={()=>setModal("")} onSaved={async()=>{await refreshState();showProjects(false);}} />}
      {modal === "project" && (
        <ProjectEditor
          project={editing as Project | undefined}
          styles={state.styles}
          onClose={() => setModal("")}
          onSaved={async (saved) => { await refreshState(); chooseProject(saved); }}
        />
      )}{" "}
      {modal === "style" && (
        <StyleEditor
          style={editing}
          onClose={() => setModal("")}
          onSaved={refreshState}
        />
      )}{" "}
      {modal === "assistant-result" && typeof editing?.result_text === "string" && <AssistantReview job={editing} projectId={projectId} draft={draft} onClose={()=>setModal("")} onApply={next=>{setDraft(next);setModal("");setJobsOpen(false);}}/>}
      {modal === "style-preview" && editing && (
        <StylePreview
          prepared={editing as PreparedStylePreview}
          state={state}
          onClose={() => setModal("")}
          onSaved={async () => { await refreshState(); setNotice(de.stylePreviewApplied); }}
          onMode={(mode) => void saveMode(mode)}
        />
      )}
      {modal === "character" && project && (
        <CharacterEditor
          character={editing}
          projectId={project.id}
          assets={assets}
          onClose={() => setModal("")}
          onSaved={refreshState}
        />
      )}{" "}
      {modal === "settings" && (
        <SettingsEditor
          state={state}
          onClose={() => setModal("")}
          onSaved={refreshState}
        />
      )}{" "}
      {modal === "export" && project && (
        <ExportEditor
          project={project}
          assets={assets}
          selected={selected}
          initialFormat={editing?.format}
          seriesId={editing?.seriesId}
          settings={state.settings}
          onClose={() => setModal("")}
        />
      )}{" "}
      {modal === "asset" && editing && (
        <AssetEditor
          asset={editing as Asset}
          onClose={() => setModal("")}
          onSaved={refreshAssets}
          onVariant={() => {
            useAsset(editing as Asset, true);
            setModal("");
          }}
        />
      )}
    </div>
  );
}
function ShieldIcon() {
  return <span className="local-lock">⌑</span>;
}
function Library({
  section,
  state,
  projectId,
  onEdit,
  onDuplicate,
  onPreview,
  onRemovePreview,
  previewBusyId,
}: {
  section: "styles" | "characters";
  state: AppState;
  projectId: string;
  onEdit: (kind: string, item?: RecordData) => void;
  onDuplicate: (id: string) => void;
  onPreview: (item: RecordData) => void;
  onRemovePreview: (item: RecordData) => void;
  previewBusyId: string;
}) {
  const styles = section === "styles";
  const items = styles
    ? state.styles
    : state.characters.filter((c) => c.project_id === projectId);
  return (
    <div className="library">
      <div className="library-heading">
        <span className="eyebrow">{t("DEINE VISUELLE SPRACHE")}</span>
        <h2>{styles ? de.styles : de.characters}</h2>
        <p>
          {styles
            ? t("Wiederverwendbare Eigenschaften für Bilder, die zusammengehören. Jede Änderung wird versioniert.")
            : t("Wiedererkennbare Merkmale und freigegebene Referenzen für deine Charaktersets.")}
        </p>
        <button
          className="button primary"
          disabled={!styles && !projectId}
          onClick={() => onEdit(styles ? "style" : "character")}
        >
          <Plus size={16} />
          {styles ? de.newStyle : de.newCharacter}
        </button>
      </div>
      <div className="library-grid">
        {items.map((item, index) => { const previewId = item.preview_asset_id || item.candidate_asset_id; const bundled = !item.preview_hidden && bundledPreviews.find(p=>p.style_id === item.id && p.style_version === item.version && Object.entries(p.blocks).every(([k,v])=>item.blocks?.[k]===v)); return (
          <article className="library-card" key={item.id}>
            <div className={`library-card-symbol tone-${index % 5}${styles ? " style-card-image" : ""}${styles && (previewId || bundled) ? " has-preview" : ""}`}>
              {styles ? (
                previewId ? <AssetImage id={previewId} alt={`${de.stylePreviewTitle}: ${item.name}`} /> : bundled ? <img className="bundled-style-image" src={bundled.file} alt={`${de.bundledPreview}: ${item.name}`} /> : <div className="style-card-empty"><Palette size={30} strokeWidth={1} /><span>{de.stylePreviewEmpty}</span></div>
              ) : item.portrait_asset_id ? (
                <AssetImage id={item.portrait_asset_id} alt={item.name} />
              ) : (
                <UserRound size={30} strokeWidth={1} />
              )}
            </div>
            <div className="library-card-content">
              <span className="eyebrow">
                {styles
                  ? `${de.version} ${item.version || 1}`
                  : item.portrait_asset_id ? t("CHARAKTERBILD") : t("CHARAKTER")}
              </span>
              <h3>{styles ? styleName(item as {id:string;name:string}) : item.name}</h3>
              {styles && item.preview_asset_id && <div className="style-preview-badges">
                <span>{de.stylePreviewVersion} {item.preview_style_version || "—"}</span>
                {item.preview_provider === "test" && <span>{de.stylePreviewTest}</span>}
                {item.preview_style_version !== item.version && <span className="stale">{de.stylePreviewStale}</span>}
              </div>}
              {styles && !previewId && bundled && <small>{de.bundledPreview}</small>}
              {styles && !item.preview_asset_id && item.candidate_asset_id && <small>{item.candidate_provider === "test" ? de.stylePreviewTest : t("Erzeugte Vorschau")}{t("· noch nicht übernommen")}</small>}
              <p>
                {styles
                  ? Object.values(item.blocks || {})
                      .filter(Boolean)
                      .join(" · ")
                  : item.description}
              </p>
              {styles && <div className="style-preview-actions">
                <button className="button small" disabled={Boolean(previewBusyId)} onClick={() => onPreview(item)}>{previewBusyId === item.id ? de.preparing : de.stylePreviewCreate}</button>
                {item.preview_asset_id && <button className="text-button" onClick={() => onRemovePreview(item)}>{de.stylePreviewRemove}</button>}
              </div>}
              <div className="button-row">
                <button
                  className="button small"
                  onClick={() => onEdit(styles ? "style" : "character", item)}
                >
                  {de.edit}
                  <ArrowRight size={14} />
                </button>
                {styles && (
                  <button
                    className="icon-button"
                    aria-label={`${de.duplicate}: ${item.name}`}
                    onClick={() => onDuplicate(item.id)}
                  >
                    <Copy size={16} />
                  </button>
                )}
              </div>
            </div>
          </article>
        );})}
      </div>
      {!items.length && (
        <Empty icon={<UserRound size={32} />} title={de.noCharacters} />
      )}
    </div>
  );
}
function AssetEditor({
  asset,
  onClose,
  onSaved,
  onVariant,
}: {
  asset: Asset;
  onClose: () => void;
  onSaved: () => Promise<void>;
  onVariant: () => void;
}) {
  const [value, setValue] = useState({
    id: asset.id,
    description: asset.description,
    tags: asset.tags || [],
    comment: asset.comment || "",
    status: asset.status || "ready",
  });
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  return (
    <Modal title={de.details} onClose={onClose} wide>
      <div className="editor-columns">
        <div className="detail-image">
          <AssetImage id={asset.id} thumbnail={false} alt={asset.description} />
          <div className="button-row">
            <button className="button" onClick={onVariant}>
              <Copy size={15} />
              {de.variant}
            </button>
            {isDesktop() && (
              <button
                className="button"
                onClick={() =>
                  void rpc("asset.reveal", { id: asset.id }).catch((e) =>
                    setError(message(e)),
                  )
                }
              >
                <FolderOpen size={15} />
                {de.reveal}
              </button>
            )}
          </div>
          <p className="hint">
            {asset.width} × {asset.height} · {asset.format} ·{" "}
            {asset.created_at
              ? new Date(asset.created_at).toLocaleString(locale())
              : ""}
          </p>
          <p className="hint">
            {costStatuses[asset.cost_status] || de.unknown} ·{" "}
            {money(asset.cost_microusd)}
            {asset.snapshot?.price_snapshot?.version && (
              <> · {asset.snapshot.price_snapshot.version}</>
            )}
          </p>
          <code className="asset-id">{asset.id}</code>
        </div>
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            try {
              await rpc("asset.update", value);
              await onSaved();
              onClose();
            } catch (e) {
              setError(message(e));
            } finally {
              setBusy(false);
            }
          }}
        >
          <Field label={de.description}>
            <textarea
              rows={4}
              value={value.description}
              onChange={(e) =>
                setValue({ ...value, description: e.target.value })
              }
            />
          </Field>
          <Field label={de.tags}>
            <input
              value={value.tags.join(", ")}
              onChange={(e) =>
                setValue({
                  ...value,
                  tags: e.target.value.split(",").map((t) => t.trim()),
                })
              }
            />
          </Field>
          <Field label={de.comment}>
            <textarea
              rows={3}
              value={value.comment}
              onChange={(e) => setValue({ ...value, comment: e.target.value })}
            />
          </Field>
          <Field label={de.imageStatus}>
            <select
              value={value.status}
              onChange={(e) => setValue({ ...value, status: e.target.value })}
            >
              {["ready", "reference", "draft", "approved", "rejected"].map(
                (s) => (
                  <option key={s} value={s}>
                    {statuses[s] || s}
                  </option>
                ),
              )}
            </select>
          </Field>
          <details className="metadata">
            <summary>{de.metadata}</summary>
            <pre>{JSON.stringify(asset, null, 2)}</pre>
          </details>
          <ErrorBox error={error} />
          <FormActions onClose={onClose} busy={busy} />
        </form>
      </div>
    </Modal>
  );
}
