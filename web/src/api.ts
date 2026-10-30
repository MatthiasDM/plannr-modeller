import type { Document, Entity, Op, OpResult, Vec3 } from './model';

export interface ProjectSummary { id: string; name: string; createdAt: string; updatedAt: string; version: number; entityCount: number }
export interface Project { id: string; name: string; createdAt: string; updatedAt: string; document: Document }

export interface CompiledGroup {
  material: string; name: string; color: string; opacity: number; back: boolean;
  positions: number[]; normals: number[]; indices: number[]; ids: string[]; idRanges: [number, number][];
}
export interface CompiledScene {
  version: number;
  groups: CompiledGroup[];
  edges: { positions: number[]; ids: string[] };
  guides: { id: string; point: Vec3; direction?: Vec3 | null }[];
  dimensions: { id: string; a: Vec3; b: Vec3; offset: Vec3; value: number }[];
  texts: { id: string; position: Vec3; text: string; anchor?: Vec3 | null }[];
  sections: { id: string; origin: Vec3; normal: Vec3; active: boolean }[];
  /** Centres of circles, arcs and regular polygons (the "Center" inference). */
  centers?: { id: string; point: Vec3; normal: Vec3; radius: number }[];
  bounds: { min: Vec3; max: Vec3 };
  metrics: { faces: number; edges: number; groups: number; area: number; areaByMaterial: Record<string, number>; edgeLength: number };
}
export interface OpsResponse { version: number; results: OpResult[] }
export interface PluginInfo { id: string; name: string; entityTypes: string[]; ops: { name: string; description: string }[]; script?: string | null }

/** This browser's key: on a public instance it owns the projects made here (private instances ignore it). */
export const owner: string = (() => {
  const k = 'plannr.owner';
  const given = new URLSearchParams(location.search).get('owner'); // an embedding app passes the key it manages (not stored)
  if (given && /^[A-Za-z0-9_-]{8,128}$/.test(given)) return given;
  try {
    let v = localStorage.getItem(k);
    if (!v) { v = (crypto.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}-${Math.random().toString(36).slice(2)}`).replace(/[^A-Za-z0-9-]/g, ''); localStorage.setItem(k, v); }
    return v;
  } catch { return `tmp-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`; }
})();

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = { 'X-Plannr-Owner': owner };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  const res = await fetch(`/api${path}`, {
    method, headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  let data: any = null; try { data = text ? JSON.parse(text) : null; } catch { data = { error: text }; }
  if (!res.ok) throw new Error(data?.error ?? `${res.status} ${res.statusText}`);
  return data as T;
}

export const api = {
  health: () => req<{ ok: boolean; version: string; name: string }>('GET', '/health'),
  listProjects: () => req<ProjectSummary[]>('GET', '/projects'),
  createProject: (name: string, document?: Document) => req<ProjectSummary>('POST', '/projects', { name, document }),
  getProject: (id: string) => req<Project>('GET', `/projects/${id}`),
  renameProject: (id: string, name: string) => req<ProjectSummary>('PATCH', `/projects/${id}`, { name }),
  putDocument: (id: string, document: Document) => req<{ version: number }>('PUT', `/projects/${id}/document`, document),
  deleteProject: (id: string) => req<void>('DELETE', `/projects/${id}`),
  duplicateProject: (id: string) => req<ProjectSummary>('POST', `/projects/${id}/duplicate`),
  ops: (id: string, ops: Op[], label?: string) => req<OpsResponse>('POST', `/projects/${id}/ops`, { ops, label }),
  preview: (id: string, ops: Op[]) => req<CompiledScene>('POST', `/projects/${id}/preview`, { ops }),
  undo: (id: string) => req<{ version: number; label: string }>('POST', `/projects/${id}/undo`),
  redo: (id: string) => req<{ version: number; label: string }>('POST', `/projects/${id}/redo`),
  history: (id: string) => req<{ version: number; label: string; at: string }[]>('GET', `/projects/${id}/history`),
  compile: (id: string) => req<CompiledScene>('GET', `/projects/${id}/compile`),
  entities: (id: string, q = '') => req<(Entity & { path: string[] })[]>('GET', `/projects/${id}/entities${q}`),
  plugins: () => req<PluginInfo[]>('GET', '/plugins'),
  sample: () => req<ProjectSummary>('POST', '/sample'),
  exportGlbUrl: (id: string) => `/api/projects/${id}/export.glb?owner=${owner}`,
  exportJsonUrl: (id: string) => `/api/projects/${id}/export.json?owner=${owner}`,
};

/** Live channel: change notifications in, viewport commands (screenshot, camera…) out. */
export class Live {
  private ws: WebSocket | null = null;
  private url: string;
  private closed = false;
  onChanged: (version: number, label: string, source: string) => void = () => {};
  onCommand: (name: string, args: any) => Promise<any> = async () => ({});
  onStatus: (connected: boolean) => void = () => {};
  constructor(projectId: string) {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    this.url = `${proto}://${location.host}/ws/projects/${projectId}?owner=${owner}`;
    this.connect();
  }
  private connect() {
    if (this.closed) return;
    const ws = new WebSocket(this.url); this.ws = ws;
    ws.onopen = () => { ws.send(JSON.stringify({ type: 'hello', client: 'browser', name: navigator.userAgent.split(' ').pop() })); this.onStatus(true); };
    ws.onclose = () => { this.onStatus(false); if (!this.closed) setTimeout(() => this.connect(), 1500); };
    ws.onmessage = async (ev) => {
      let m: any; try { m = JSON.parse(ev.data); } catch { return; }
      if (m.type === 'changed') this.onChanged(m.version, m.label, m.source);
      if (m.type === 'command') {
        try { const result = await this.onCommand(m.command.name, m.command); ws.send(JSON.stringify({ type: 'reply', reqId: m.reqId, result: result ?? {} })); }
        catch (e: any) { ws.send(JSON.stringify({ type: 'reply', reqId: m.reqId, error: e?.message ?? String(e) })); }
      }
    };
  }
  sendState(state: { camera: number[]; selection: string[]; tool: string; hover: string | null }) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify({ type: 'state', ...state }));
  }
  close() { this.closed = true; this.ws?.close(); }
}
