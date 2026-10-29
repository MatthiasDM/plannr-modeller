import { MOD_LABEL, type App, type ModKey } from '../main';
function loadCollapsed(): Set<string> { try { const v = JSON.parse(localStorage.getItem('plannr.trayCollapsed') ?? 'null'); if (v?.collapsed) return new Set([...v.collapsed, ...(v.known ?? []).filter((k: string) => !v.collapsed.includes(k)).map((k: string) => `!${k}`)]); } catch { /* ignore */ } return new Set(); }
import { icon } from './icons';
import { findEntity, materialOf, layerOf, walk, fmt, fmtM, fmtM2, KIND_LABEL, polygonNormal, type Entity, type FaceEntity, type Vec3 } from '../model';
import { getPlugins } from '../plugins';

const esc = (s: string) => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));
const num = (k: string, v: number, label: string, step = 0.01) => `<label>${label}</label><input type="number" step="${step}" data-field="${k}" value="${+v.toFixed(4)}">`;
const vec = (k: string, p: Vec3, label: string) => `<label>${label}</label><div class="row">${[0, 1, 2].map(i => `<input type="number" step="0.01" data-field="${k}.${i}" value="${+p[i].toFixed(4)}">`).join('')}</div>`;

export class Tray {
  /** Collapsed panel ids. Everything starts collapsed; the choice is remembered per browser. */
  collapsed: Set<string> = loadCollapsed();
  private known = new Set<string>();
  constructor(private app: App, public el: HTMLElement) {}
  private persist() { try { localStorage.setItem('plannr.trayCollapsed', JSON.stringify({ collapsed: [...this.collapsed], known: [...this.known] })); } catch { /* private mode */ } }
  private isCollapsed(id: string) { if (!this.known.has(id)) { this.known.add(id); if (!this.collapsed.has(`!${id}`)) this.collapsed.add(id); } return this.collapsed.has(id); }
  setAll(collapsed: boolean) { for (const id of this.known) { if (collapsed) this.collapsed.add(id); else { this.collapsed.delete(id); this.collapsed.add(`!${id}`); } } if (collapsed) for (const id of this.known) this.collapsed.delete(`!${id}`); this.persist(); this.render(); }

  render() {
    const a = this.app; const d = a.doc;
    const panel = (id: string, title: string, body: string, count = '') =>
      `<section class="panel ${this.isCollapsed(id) ? 'collapsed' : ''}" data-panel="${id}"><header>${icon('chevron')}<span>${title}</span><span class="count">${count}</span></header><div class="body">${body}</div></section>`;
    const allCollapsed = [...this.known].every(id => this.collapsed.has(id));
    let html = `<div class="tray-bar"><span class="muted">Panels</span><span style="flex:1"></span><button class="icon-btn" data-tray="${allCollapsed ? 'expand' : 'collapse'}" title="${allCollapsed ? 'Expand all panels' : 'Collapse all panels'}">${icon(allCollapsed ? 'expandAll' : 'collapseAll')}</button></div>` +
      panel('entity', 'Entity Info', this.entityInfo()) +
      panel('layers', 'Tags', this.layers(), `${d.layers.length}`) +
      panel('materials', 'Materials', this.materials(), `${d.materials.length}`) +
      panel('outliner', 'Outliner', this.outliner(), `${d.entities.length}`) +
      panel('components', 'Components', this.components(), `${d.components?.length ?? 0}`) +
      panel('scenes', 'Scenes', this.scenes(), `${d.scenes.length}`) +
      panel('sections', 'Section Planes', this.sections()) +
      panel('display', 'Display', this.display()) +
      panel('info', 'Model Info', this.modelInfo()) +
      panel('history', 'History', this.history()) +
      panel('shortcuts', 'Shortcuts', this.shortcuts());
    for (const p of getPlugins()) for (const pp of p.panels ?? []) html += panel(`plugin:${p.id}:${pp.id}`, pp.title, `<div data-plugin-panel="${p.id}:${pp.id}"></div>`).replace('<section class="panel', `<section style="--plugin-accent:${p.accent ?? '#8e44ad'}" class="panel plugin`).replace('<span class="count">', `<span class="plugin-tag" title="${esc(p.name)} plugin">plugin</span><span class="count">`);
    html += panel('plugins', 'Plugins', this.plugins(), `${getPlugins().length + a.serverPlugins.length}`);
    this.el.innerHTML = html;
    for (const p of getPlugins()) for (const pp of p.panels ?? []) { const host = this.el.querySelector<HTMLElement>(`[data-plugin-panel="${p.id}:${pp.id}"]`); if (host) try { pp.render(a.ctx, host); } catch (e) { host.textContent = `Panel error: ${(e as Error).message}`; } }
    const pe = this.el.querySelector<HTMLElement>('[data-plugin-entity]');
    if (pe) { const p = getPlugins().find(p => p.id === pe.dataset.pluginEntity); const ent = findEntity(a.doc, [...a.selection][0]); if (p?.renderEntity && ent) try { if (!p.renderEntity(a.ctx, ent, pe)) pe.innerHTML = `<div class="muted">${ent.kind === 'custom' ? ent.type : ''}</div>`; } catch (err) { pe.textContent = `Error: ${(err as Error).message}`; } }
    this.bind();
  }

  private entityInfo(): string {
    const a = this.app; const ids = [...a.selection];
    if (!ids.length) return `<div class="empty">Nothing selected. Click something with the Select tool.</div>`;
    if (ids.length > 1) {
      const kinds = ids.map(id => findEntity(a.doc, id)?.kind ?? '?');
      return `<div>${ids.length} entities selected <span class="muted">(${[...new Set(kinds)].join(', ')})</span></div>
        <div class="btn-row"><button class="btn" data-act="group">${icon('group')} Make group</button><button class="btn" data-act="delete">${icon('trash')} Erase</button></div>`;
    }
    const e = findEntity(a.doc, ids[0]); if (!e) return `<div class="empty">Unknown entity</div>`;
    if (e.kind === 'custom') { const pid = e.type.split('.')[0]; const p = getPlugins().find(p => p.id === pid); if (p?.renderEntity) return `<div data-plugin-entity="${pid}"></div>`; }
    const layerSel = `<label>Tag</label><select data-field="layer">${a.doc.layers.map(l => `<option value="${l.id}" ${l.id === e.layer ? 'selected' : ''}>${esc(l.name)}</option>`).join('')}</select>`;
    const matSel = (k: 'material' | 'backMaterial', label: string, cur?: string | null) => `<label>${label}</label><select data-field="${k}"><option value="">(default)</option>${a.doc.materials.map(m => `<option value="${m.id}" ${m.id === cur ? 'selected' : ''}>${esc(m.name)}</option>`).join('')}</select>`;
    let body = `<div class="kv"><label>Type</label><div><b>${KIND_LABEL[e.kind]}</b> <span class="muted">${e.id.slice(0, 8)}</span></div>${layerSel}`;
    switch (e.kind) {
      case 'face': { const f = e as FaceEntity; const n = polygonNormal(f.outer);
        body += `${matSel('material', 'Material', f.material)}${matSel('backMaterial', 'Back material', f.backMaterial)}<label>Vertices</label><div>${f.outer.length}${f.holes?.length ? ` + ${f.holes.length} hole(s)` : ''}</div><label>Normal</label><div class="muted">${n.map(x => x.toFixed(2)).join(', ')}</div><label>Area</label><div>${fmtM2(this.faceArea(f))}</div>`; break; }
      case 'edge': body += `${vec('a', e.a, 'Start')}${vec('b', e.b, 'End')}<label>Length</label><div>${fmtM(Math.hypot(e.b[0] - e.a[0], e.b[1] - e.a[1], e.b[2] - e.a[2]))}</div>`; break;
      case 'group': { const n = [...walk(e.entities)].length; body += `<label>Name</label><input type="text" data-field="name" value="${esc(e.name)}"><label>Contents</label><div>${n} entities</div>`; break; }
      case 'guide': body += `${vec('point', e.point, 'Point')}${e.direction ? vec('direction', e.direction, 'Direction') : ''}`; break;
      case 'dimension': body += `${vec('a', e.a, 'A')}${vec('b', e.b, 'B')}${vec('offset', e.offset, 'Offset')}<label>Value</label><div>${fmtM(Math.hypot(e.b[0] - e.a[0], e.b[1] - e.a[1], e.b[2] - e.a[2]))}</div>`; break;
      case 'text': body += `<label>Text</label><input type="text" data-field="text" value="${esc(e.text)}">${vec('position', e.position, 'Position')}`; break;
      case 'section': { const n = e.normal; const tilt = Math.acos(Math.max(-1, Math.min(1, n[1]))) * 180 / Math.PI; const heading = (Math.atan2(n[0], n[2]) * 180 / Math.PI + 360) % 360;
        body += `${vec('origin', e.origin, 'Origin')}<label>Tilt</label><div class="row"><input type="number" step="1" min="0" max="180" data-sec="tilt" value="${tilt.toFixed(1)}" title="0° = horizontal cut, 90° = vertical"><input type="range" min="0" max="180" step="1" data-sec-range="tilt" value="${tilt.toFixed(0)}" style="width:70px"></div>
          <label>Heading</label><div class="row"><input type="number" step="1" min="0" max="360" data-sec="heading" value="${heading.toFixed(1)}" title="Direction the cut-away side faces, 0° = +z"><input type="range" min="0" max="360" step="1" data-sec-range="heading" value="${heading.toFixed(0)}" style="width:70px"></div>
          <label>Offset</label><div class="row"><input type="number" step="0.05" data-sec="offset" value="0" title="Push the plane along its normal (m), Enter applies"></div>
          <label>Active</label><input type="checkbox" data-field="active" ${e.active ? 'checked' : ''}><label></label><div class="muted" style="font-size:11px">Use Move (axis locks ← → ↑) and Rotate on the plane itself too.</div>
          <label>2D slice</label><div><button class="btn" data-slice="${e.id}" title="The cut lines of the whole model at this plane, as a new group of edges">${icon('slice')}Create slice</button></div>`; break; }
      case 'instance': { const def = (a.doc.components ?? []).find(c => c.id === e.definition); const n = [...walk(a.doc.entities)].filter(x => x.entity.kind === 'instance' && (x.entity as any).definition === e.definition).length;
        body += `<label>Definition</label><div><b>${esc(def?.name ?? '?')}</b> <span class="muted">· ${n} instance${n === 1 ? '' : 's'}</span></div><label>Contents</label><div>${def ? [...walk(def.entities)].length : 0} entities</div><label>Name</label><input type="text" data-field="name" value="${esc(e.name ?? '')}" placeholder="(instance name)">`; break; }
      case 'custom': body += `<label>Plugin type</label><div>${esc(e.type)}</div><label>Data</label><div class="muted" style="word-break:break-all;font-size:11px">${esc(JSON.stringify(e.data)).slice(0, 300)}</div>`; break;
    }
    body += `</div><div class="btn-row">`;
    if (e.kind === 'group') body += `<button class="btn" data-act="explode">Explode</button><button class="btn" data-act="enter">Edit group</button>`;
    else if (e.kind === 'instance') body += `<button class="btn" data-act="placeAnother">Place another</button><button class="btn" data-act="makeUnique">Make unique</button><button class="btn" data-act="explode">Explode</button>`;
    else body += `<button class="btn" data-act="group">${icon('group')} Make group</button>`;
    body += `<button class="btn" data-act="delete">${icon('trash')} Erase</button></div>`;
    return body;
  }
  private faceArea(f: FaceEntity) {
    const n = polygonNormal(f.outer); const cross = (pts: Vec3[]) => { let s: Vec3 = [0, 0, 0]; for (let i = 0; i < pts.length; i++) { const a = pts[i], b = pts[(i + 1) % pts.length]; s = [s[0] + a[1] * b[2] - a[2] * b[1], s[1] + a[2] * b[0] - a[0] * b[2], s[2] + a[0] * b[1] - a[1] * b[0]]; } return Math.abs(s[0] * n[0] + s[1] * n[1] + s[2] * n[2]) / 2; };
    return cross(f.outer) - (f.holes ?? []).reduce((s, h) => s + cross(h), 0);
  }

  private layers(): string {
    const a = this.app;
    return a.doc.layers.map(l => `<div class="layer-row ${l.visible ? '' : 'off'} ${a.options.layer === l.id ? 'active' : ''}"><button class="icon-btn" data-layer-vis="${l.id}">${icon(l.visible ? 'eye' : 'eyeOff')}</button><input type="color" data-layer-color="${l.id}" value="${l.color}" style="width:18px;height:18px;border:0;padding:0;background:none"><span class="name" data-layer-use="${l.id}" title="Click to draw on this tag">${esc(l.name)}${a.options.layer === l.id ? ' <span class="muted">· active</span>' : ''}</span>${a.doc.layers.length > 1 ? `<button class="icon-btn" data-layer-del="${l.id}">${icon('trash')}</button>` : ''}</div>`).join('')
      + `<div class="btn-row"><button class="btn" data-act="addLayer">${icon('plus')} Add tag</button></div>`;
  }
  private materials(): string {
    const a = this.app;
    return `<div class="materials">${a.doc.materials.map(m => `<div class="material ${a.options.material === m.id ? 'active' : ''}" data-material="${m.id}" data-label="${esc(m.name)}" style="background:${m.color};opacity:${Math.max(0.3, m.opacity)}"></div>`).join('')}<div class="material" data-act="addMaterial" data-label="New material" style="display:grid;place-items:center;background:#fff">${icon('plus')}</div></div>
      ${(() => { const m = materialOf(a.doc, a.options.material); return m ? `<div class="kv" style="margin-top:8px"><label>Name</label><input type="text" data-mat="name" value="${esc(m.name)}"><label>Colour</label><input type="color" data-mat="color" value="${m.color}"><label>Opacity</label><input type="range" min="0.05" max="1" step="0.05" data-mat="opacity" value="${m.opacity}"></div>` : ''; })()}`;
  }
  private outliner(): string {
    const a = this.app; const rows: string[] = [];
    const walkTree = (ents: Entity[], depth: number) => {
      for (const e of ents) {
        if (e.kind === 'edge' && depth === 0 && a.doc.entities.length > 60) continue;
        const name = e.kind === 'group' ? e.name : e.kind === 'instance' ? `${(a.doc.components ?? []).find(c => c.id === e.definition)?.name ?? 'component'}${e.name ? ` (${e.name})` : ''}` : e.kind === 'text' ? `"${e.text}"` : e.kind === 'custom' ? `${(e.data as any)?.name ?? (e.data as any)?.kind ?? (e.data as any)?.style ?? ''} ${e.type.split('.').pop()}`.trim() : KIND_LABEL[e.kind];
        rows.push(`<div class="list-row ${a.selection.has(e.id) ? 'active' : ''}" style="--depth:${depth}" data-select="${e.id}"><span class="name">${e.kind === 'group' ? icon('group') : e.kind === 'instance' ? icon('component') : ''}${esc(name)}</span><span class="muted">${layerOf(a.doc, e.layer)?.name ?? ''}</span></div>`);
        if (e.kind === 'group') walkTree(e.entities, depth + 1);
      }
    };
    walkTree(a.doc.entities, 0);
    if (!rows.length) return `<div class="empty">Empty model — draw something.</div>`;
    return `<div class="entity-tree">${rows.slice(0, 300).join('')}${rows.length > 300 ? `<div class="muted">… ${rows.length - 300} more</div>` : ''}</div>`;
  }
  private components(): string {
    const a = this.app; const defs = a.doc.components ?? []; if (!defs.length) return `<div class="empty">No components. Select entities and use Edit → Make Component (also an MCP tool).</div>`;
    const counts = new Map<string, number>(); for (const { entity } of walk(a.doc.entities)) if (entity.kind === 'instance') counts.set(entity.definition, (counts.get(entity.definition) ?? 0) + 1);
    return defs.map(d => `<div class="list-row" data-def="${d.id}"><span class="name">${icon('component')}${esc(d.name)} <span class="muted">· ${counts.get(d.id) ?? 0}×</span></span><button class="icon-btn" data-def-place="${d.id}" title="Place an instance">${icon('place')}</button><button class="icon-btn" data-def-rename="${d.id}" title="Rename">✎</button><button class="icon-btn" data-def-del="${d.id}" title="Remove definition">${icon('trash')}</button></div>`).join('');
  }
  private scenes(): string {
    const a = this.app;
    return a.doc.scenes.map(s => `<div class="list-row" data-scene="${s.id}"><span class="name">${esc(s.name)}</span><button class="icon-btn" data-scene-update="${s.id}" title="Update to current view">${icon('camera')}</button><button class="icon-btn" data-scene-del="${s.id}">${icon('trash')}</button></div>`).join('')
      + `<div class="btn-row"><button class="btn" data-act="addScene">${icon('plus')} Add scene</button></div>`;
  }
  private sections(): string {
    const a = this.app; const secs = [...walk(a.doc.entities)].map(x => x.entity).filter(e => e.kind === 'section');
    if (!secs.length) return `<div class="empty">No section planes. Use the Section Plane tool.</div>`;
    return secs.map(s => s.kind === 'section' ? `<div class="layer-row"><button class="icon-btn" data-section-toggle="${s.id}">${icon(s.active ? 'eye' : 'eyeOff')}</button><span class="name" data-select="${s.id}">Section · ${s.normal.map(x => x.toFixed(1)).join(',')} @ ${s.origin.map(x => x.toFixed(2)).join(', ')}</span><button class="icon-btn" data-slice="${s.id}" title="Create a 2D slice: the cut lines at this plane, as a group">${icon('slice')}</button><button class="icon-btn" data-erase="${s.id}">${icon('trash')}</button></div>` : '').join('');
  }
  private display(): string {
    const a = this.app; const row = (k: string, label: string, on: boolean) => `<div class="layer-row"><button class="icon-btn" data-display="${k}">${icon(on ? 'eye' : 'eyeOff')}</button><span class="name">${label}</span></div>`;
    return row('grid', 'Grid', a.display.grid) + row('axes', 'Axes', a.display.axes) + row('edges', 'Edges', a.display.edges) + row('figure', 'Scale figure', a.display.figure) + row('shadows', 'Shadows', a.display.shadows) +
      `<div class="layer-row"><input type="checkbox" data-opt="gridSnap" ${a.options.gridSnap ? 'checked' : ''}><span class="name">Snap to 1 cm grid</span></div>`;
  }
  private modelInfo(): string {
    const m = this.app.scene.compiled?.metrics; if (!m) return `<div class="empty">Compiling…</div>`;
    const stat = (v: string, l: string) => `<div class="stat"><div class="v">${v}</div><div class="l">${l}</div></div>`;
    return `<div class="stats">${stat(String(m.faces), 'Faces')}${stat(String(m.edges), 'Edges')}${stat(String(m.groups), 'Groups')}${stat(fmtM2(m.area), 'Face area')}${stat(fmtM(m.edgeLength), 'Edge length')}${stat(`v${this.app.doc.version}`, 'Version')}</div>
      ${Object.keys(m.areaByMaterial).length ? `<table class="data"><tr><th>Material</th><th>Area</th></tr>${Object.entries(m.areaByMaterial).sort((x, y) => y[1] - x[1]).map(([k, v]) => `<tr><td>${esc(k)}</td><td>${fmt(v, 2)} m²</td></tr>`).join('')}</table>` : ''}`;
  }
  private history(): string {
    const h = this.app.historyList; if (!h.length) return `<div class="empty">No changes yet.</div>`;
    return `<div class="hist">${h.slice(-30).reverse().map(x => `<div class="${x.version === this.app.doc.version ? 'cur' : ''}">v${x.version} · ${esc(x.label)}</div>`).join('')}</div>`;
  }
  private shortcuts(): string {
    const a = this.app; const keys: ModKey[] = ['Control', 'Alt', 'Shift'];
    const opts = (cur: string) => `<option value="" ${cur ? '' : 'selected'}>— none —</option>` + a.tools.filter(t => t.id !== 'select').map(t => `<option value="${t.id}" ${t.id === cur ? 'selected' : ''}>${esc(t.label)}</option>`).join('');
    return `<div class="muted" style="margin:4px 0 6px">Hold a key to use a tool; release it to return to the tool you had.</div>
      <div class="kv">${keys.map(k => `<label>Hold ${MOD_LABEL[k]}</label><select data-modtool="${k}">${opts(a.modifierTools[k])}</select>`).join('')}</div>
      <div class="muted" style="margin-top:8px;font-size:11px">Shift keeps its usual meaning inside tools (add to selection) unless you assign it. Middle mouse always orbits, Shift+middle pans, the wheel zooms.</div>`;
  }
  private plugins(): string {
    const a = this.app; const fp = getPlugins(); const sp = a.serverPlugins;
    if (!fp.length && !sp.length) return `<div class="muted">No plugins loaded. Browser plugins call <code>window.plannr.registerPlugin(…)</code>; engine plugins are registered in <code>crates/plannr-server/src/plugins.rs</code>. See docs/PLUGINS.md.</div>`;
    return fp.map(p => `<div class="list-row"><span class="name">${icon('plugin')} ${esc(p.name)} <span class="muted">browser · ${(p.tools ?? []).length} tools</span></span></div>`).join('') + sp.map(p => `<div class="list-row"><span class="name">${icon('plugin')} ${esc(p.name)} <span class="muted">engine · ${p.ops.length} ops</span></span></div>`).join('');
  }

  private bind() {
    const a = this.app; const el = this.el;
    el.querySelectorAll<HTMLElement>('.panel > header').forEach(hd => hd.onclick = () => { const id = hd.parentElement!.dataset.panel!; if (this.collapsed.has(id)) { this.collapsed.delete(id); this.collapsed.add(`!${id}`); } else { this.collapsed.add(id); this.collapsed.delete(`!${id}`); } this.persist(); this.render(); });
    el.querySelector<HTMLElement>('[data-tray]')?.addEventListener('click', (e) => this.setAll((e.currentTarget as HTMLElement).dataset.tray === 'collapse'));
    el.querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-field]').forEach(inp => inp.onchange = () => {
      const id = [...a.selection][0]; const e = findEntity(a.doc, id); if (!e) return;
      const path = inp.dataset.field!.split('.');
      const val: any = inp.type === 'checkbox' ? (inp as HTMLInputElement).checked : inp.type === 'number' ? parseFloat(inp.value) : inp.value || null;
      const patch: Record<string, unknown> = {};
      if (path.length === 2) { const arr = [...((e as any)[path[0]] as number[])]; arr[+path[1]] = val; patch[path[0]] = arr; } else patch[path[0]] = val;
      if (e.kind === 'section' && 'active' in patch) a.commitOps([{ op: 'update_section', id, active: !!patch.active }], 'Section');
      else a.commitOps([{ op: 'update_entity', id, patch }], 'Edit');
    });
    el.querySelector<HTMLElement>('[data-act="delete"]')?.addEventListener('click', () => a.deleteSelection());
    // section plane: tilt / heading / offset
    const secApply = () => {
      const id = [...a.selection][0]; const e = findEntity(a.doc, id); if (!e || e.kind !== 'section') return;
      const tilt = parseFloat(el.querySelector<HTMLInputElement>('[data-sec="tilt"]')!.value) * Math.PI / 180, heading = parseFloat(el.querySelector<HTMLInputElement>('[data-sec="heading"]')!.value) * Math.PI / 180;
      const normal: Vec3 = [+(Math.sin(tilt) * Math.sin(heading)).toFixed(5), +Math.cos(tilt).toFixed(5), +(Math.sin(tilt) * Math.cos(heading)).toFixed(5)];
      const off = parseFloat(el.querySelector<HTMLInputElement>('[data-sec="offset"]')!.value) || 0;
      const origin: Vec3 = [e.origin[0] + e.normal[0] * off, e.origin[1] + e.normal[1] * off, e.origin[2] + e.normal[2] * off];
      a.commitOps([{ op: 'update_section', id, normal, origin: off ? origin : undefined } as any], 'Section plane');
    };
    el.querySelectorAll<HTMLInputElement>('[data-sec]').forEach(i => i.onchange = secApply);
    el.querySelectorAll<HTMLInputElement>('[data-sec-range]').forEach(r => { r.oninput = () => { el.querySelector<HTMLInputElement>(`[data-sec="${r.dataset.secRange}"]`)!.value = r.value; }; r.onchange = secApply; });
    el.querySelector<HTMLElement>('[data-act="group"]')?.addEventListener('click', () => a.groupSelection());
    el.querySelector<HTMLElement>('[data-act="explode"]')?.addEventListener('click', () => a.commitOps([{ op: 'explode', id: [...a.selection][0] }], 'Explode').then(() => a.select([])));
    el.querySelector<HTMLElement>('[data-act="enter"]')?.addEventListener('click', () => a.enterGroup([...a.selection][0]));
    el.querySelector<HTMLElement>('[data-act="placeAnother"]')?.addEventListener('click', () => { const e = findEntity(a.doc, [...a.selection][0]); if (e?.kind === 'instance') a.placeComponent(e.definition); });
    el.querySelector<HTMLElement>('[data-act="makeUnique"]')?.addEventListener('click', () => a.commitOps([{ op: 'make_unique', instance: [...a.selection][0] }], 'Make unique'));
    el.querySelectorAll<HTMLElement>('[data-def]').forEach(r => r.onclick = (e) => { if ((e.target as HTMLElement).closest('button')) return; const ids = [...walk(a.doc.entities)].filter(x => x.entity.kind === 'instance' && (x.entity as any).definition === r.dataset.def).map(x => x.entity.id); a.select(ids); });
    el.querySelectorAll<HTMLElement>('[data-def-place]').forEach(b => b.onclick = () => a.placeComponent(b.dataset.defPlace!));
    el.querySelectorAll<HTMLElement>('[data-def-rename]').forEach(b => b.onclick = async () => { const d = (a.doc.components ?? []).find(c => c.id === b.dataset.defRename)!; const n = await a.promptText('Component name', d.name); if (n) a.commitOps([{ op: 'rename_component', definition: d.id, name: n }], 'Rename component'); });
    el.querySelectorAll<HTMLElement>('[data-def-del]').forEach(b => b.onclick = async () => { if (await a.confirm('Remove this component definition and all its instances?', 'Remove', true)) a.commitOps([{ op: 'remove_component', definition: b.dataset.defDel!, force: true }], 'Remove component'); });
    el.querySelectorAll<HTMLElement>('[data-layer-vis]').forEach(b => b.onclick = () => { const l = layerOf(a.doc, b.dataset.layerVis!)!; a.commitOps([{ op: 'update_layer', id: l.id, visible: !l.visible }], 'Tag visibility'); });
    el.querySelectorAll<HTMLInputElement>('[data-layer-color]').forEach(i => i.onchange = () => a.commitOps([{ op: 'update_layer', id: i.dataset.layerColor!, color: i.value }], 'Tag colour'));
    el.querySelectorAll<HTMLElement>('[data-layer-use]').forEach(s => { s.onclick = () => { a.options.layer = s.dataset.layerUse!; this.render(); }; s.ondblclick = async () => { const l = layerOf(a.doc, s.dataset.layerUse!)!; const n = await a.promptText('Tag name', l.name); if (n) a.commitOps([{ op: 'update_layer', id: l.id, name: n }], 'Rename tag'); }; });
    el.querySelectorAll<HTMLElement>('[data-layer-del]').forEach(b => b.onclick = () => a.commitOps([{ op: 'remove_layer', id: b.dataset.layerDel! }], 'Remove tag').then(() => { if (a.options.layer === b.dataset.layerDel) a.options.layer = a.doc.layers[0].id; }));
    el.querySelector<HTMLElement>('[data-act="addLayer"]')?.addEventListener('click', async () => { const n = await a.promptText('Tag name', `Tag ${a.doc.layers.length}`); if (n) a.commitOps([{ op: 'add_layer', name: n }], 'Add tag'); });
    el.querySelectorAll<HTMLElement>('[data-material]').forEach(m => m.onclick = () => { a.options.material = m.dataset.material!; this.render(); });
    el.querySelector<HTMLElement>('[data-act="addMaterial"]')?.addEventListener('click', async () => { const n = await a.promptText('Material name', `Material ${a.doc.materials.length + 1}`); if (!n) return; const r = await a.commitOps([{ op: 'add_material', name: n, color: '#' + Math.floor(Math.random() * 0xffffff).toString(16).padStart(6, '0') }], 'Add material'); if (r[0]?.created?.[0]) { a.options.material = r[0].created[0]; this.render(); } });
    el.querySelectorAll<HTMLInputElement>('[data-mat]').forEach(i => i.onchange = () => { const k = i.dataset.mat!; a.commitOps([{ op: 'update_material', id: a.options.material, [k]: k === 'opacity' ? parseFloat(i.value) : i.value } as any], 'Material'); });
    el.querySelectorAll<HTMLElement>('[data-select]').forEach(r => { r.onclick = () => a.select([r.dataset.select!]); r.ondblclick = () => { a.select([r.dataset.select!]); a.zoomSelection(); }; });
    el.querySelectorAll<HTMLElement>('[data-scene]').forEach(r => r.onclick = (e) => { if ((e.target as HTMLElement).closest('button')) return; a.restoreScene(r.dataset.scene!); });
    el.querySelectorAll<HTMLElement>('[data-scene-update]').forEach(b => b.onclick = () => a.updateScene(b.dataset.sceneUpdate!));
    el.querySelectorAll<HTMLElement>('[data-scene-del]').forEach(b => b.onclick = () => a.removeScene(b.dataset.sceneDel!));
    el.querySelector<HTMLElement>('[data-act="addScene"]')?.addEventListener('click', () => a.saveScene());
    el.querySelectorAll<HTMLElement>('[data-slice]').forEach(b => b.onclick = (ev) => { ev.stopPropagation(); a.sliceSection(b.dataset.slice!); });
    el.querySelectorAll<HTMLElement>('[data-section-toggle]').forEach(b => b.onclick = () => { const s = findEntity(a.doc, b.dataset.sectionToggle!); if (s?.kind === 'section') a.commitOps([{ op: 'update_section', id: s.id, active: !s.active }], 'Section'); });
    el.querySelectorAll<HTMLElement>('[data-erase]').forEach(b => b.onclick = () => a.commitOps([{ op: 'erase', ids: [b.dataset.erase!] }], 'Erase'));
    el.querySelectorAll<HTMLElement>('[data-display]').forEach(b => b.onclick = () => a.toggleDisplay(b.dataset.display as any));
    el.querySelector<HTMLInputElement>('[data-opt="gridSnap"]')?.addEventListener('change', (e) => { a.options.gridSnap = (e.target as HTMLInputElement).checked; });
    el.querySelectorAll<HTMLSelectElement>('[data-modtool]').forEach(s => s.onchange = () => a.setModifierTool(s.dataset.modtool as ModKey, s.value));
  }
}
