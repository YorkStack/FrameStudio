export type RecordData = Record<string, any>;
export type Project = RecordData & {
  id: string;
  name: string;
  description: string;
  archived?: boolean;
  style_id?: string;
  aspect_ratio?: string;
};
export type Asset = RecordData & {
  id: string;
  description: string;
  width: number;
  height: number;
  format: string;
  favorite?: boolean;
  tags?: string[];
  status?: string;
  created_at?: string;
};
export type Job = RecordData & {
  id: string;
  status: string;
  error?: string;
  description?: string;
};
export type Model = RecordData & {
  id: string;
  name?: string;
  provider: string;
  aspect_ratios?: string[];
  resolutions?: string[];
  qualities?: string[];
  default_quality?: string;
};
export type AppState = {
  app_version?: string;
  projects: Project[];
  styles: RecordData[];
  characters: RecordData[];
  settings: RecordData;
  models: Model[];
  key_status: any;
  data_dir: string;
  pricing_revision?: string;
};
export type Variant = {
  view: string;
  pose: string;
  expression: string;
  mouth: string;
};
export type Draft = {
  description: string;
  style_id: string;
  character_id: string;
  reference_ids: string[];
  parent_id?: string;
  provider: string;
  model: string;
  aspect_ratio: string;
  resolution: string;
  quality?: string;
  count: number;
  blocks: Record<string, string>;
  variants: Variant[];
};
