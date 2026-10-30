import { THEMES, type App, type Theme } from '../main';
const THEME_LABEL: Record<Theme, string> = { system: 'System', light: 'Light', dark: 'Dark' };
import { icon } from './icons';
import { TOOLBAR, EMBED_TOOLS } from '../tools';
import { getPlugins } from '../plugins';
import type { ProjectSummary } from '../api';

const esc = (s: string) => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));
const KEY = navigator.platform.includes('Mac') ? '⌘' : 'Ctrl+';
type MenuItem = { label: string; key?: string; act: () => void; disabled?: boolean } | 'sep';

export function renderMenubar(app: App, el: HTMLElement) {
  const tools = app.tools;
  const menus: [string, () => MenuItem[]][] = [
    ['File', () => [
      { label: 'New…', key: `${KEY}N`, act: () => app.showWelcome() }, { label: 'Open…', key: `${KEY}O`, act: () => app.showWelcome() }, 'sep',
      { label: 'Duplicate project', act: () => app.duplicate() }, { label: 'Rename…', act: () => app.renamePrompt() }, 'sep',
      { label: 'Import document JSON…', act: () => app.importJson() }, { label: 'Export 3D model (.glb)', act: () => app.exportGlb() }, { label: 'Export document JSON', act: () => app.exportJson() }, 'sep',
      { label: 'Load sample model', act: () => app.loadSample() }, { label: 'Delete project…', act: () => app.deleteProject() },
    ]],
    ['Edit', () => [
      { label: 'Undo', key: `${KEY}Z`, act: () => app.undo(), disabled: !app.canUndo }, { label: 'Redo', key: `⇧${KEY}Z`, act: () => app.redo(), disabled: !app.canRedo }, 'sep',
      { label: 'Erase', key: '⌫', act: () => app.deleteSelection(), disabled: !app.selection.size },
      { label: 'Select all', key: `${KEY}A`, act: () => app.selectAll() }, { label: 'Select none', act: () => app.select([]) }, 'sep',
      { label: 'Make group', key: 'G', act: () => app.groupSelection(), disabled: !app.selection.size },
      { label: 'Make Component…', act: () => app.makeComponent(), disabled: !app.selection.size },
      { label: 'Explode', act: () => app.explodeSelection(), disabled: !app.selection.size },
      { label: 'Close group', key: 'Esc', act: () => app.exitGroup(), disabled: !app.enteredGroup },
    ]],
    ['View', () => [
      ...(['grid', 'axes', 'edges', 'figure', 'shadows'] as const).map(k => ({ label: `${app.display[k] ? '✓ ' : '   '}${({ grid: 'Grid', axes: 'Axes', edges: 'Edges', figure: 'Scale figure', shadows: 'Shadows' })[k]}`, act: () => app.toggleDisplay(k) })),
      'sep', ...app.doc.layers.map(l => ({ label: `${l.visible ? '✓ ' : '   '}Tag: ${l.name}`, act: () => app.commitOps([{ op: 'update_layer', id: l.id, visible: !l.visible }], 'Tag visibility') })),
      'sep', ...(['left', 'top'] as const).map(d => ({ label: `${app.paletteDock === d ? '✓ ' : '   '}Tools on the ${d}`, act: () => app.dockPalette(d) })),
      { label: '   Reset tool palette size', act: () => app.resetPaletteSize() },
      'sep', ...THEMES.map(t => ({ label: `${app.theme === t ? '✓ ' : '   '}Theme: ${THEME_LABEL[t]}`, act: () => app.setTheme(t) })),
    ]],
    ['Camera', () => [
      { label: 'Zoom Extents', key: '⇧Z', act: () => app.zoomExtents() }, { label: 'Zoom Selection', act: () => app.zoomSelection(), disabled: !app.selection.size }, { label: 'Zoom Window', act: () => app.setTool('zoomwin') }, 'sep',
      { label: 'Previous view', key: '⌘[', act: () => app.previousView(), disabled: !app.canPrevView }, { label: 'Next view', key: '⌘]', act: () => app.nextView(), disabled: !app.canNextView }, 'sep',
      ...(['iso', 'top', 'bottom', 'front', 'back', 'left', 'right'] as const).map(v => ({ label: `Standard view · ${v[0].toUpperCase()}${v.slice(1)}`, act: () => app.setView(v) })),
      'sep', { label: 'Add scene…', act: () => app.saveScene() },
    ]],
    ['Draw', () => tools.filter(t => ['line', 'rectangle', 'rotrect', 'circle', 'polygon', 'arc', 'arc3', 'arc2', 'pie'].includes(t.id)).map(t => ({ label: t.label, key: t.shortcut?.toUpperCase(), act: () => app.setTool(t.id) }))],
    ['Tools', () => tools.filter(t => !['line', 'rectangle', 'rotrect', 'circle', 'polygon', 'arc', 'arc3', 'arc2', 'pie'].includes(t.id) && !getPlugins().some(p => p.tools?.includes(t))).map(t => ({ label: t.label, key: t.shortcut?.trim() ? t.shortcut.toUpperCase() : t.id === 'select' ? 'Space' : undefined, act: () => app.setTool(t.id) }))],
    ['Plugins', () => {
      const items: MenuItem[] = getPlugins().flatMap(p => [{ label: `● ${p.name}`, act: () => {}, disabled: true }, ...(p.tools ?? []).map(t => ({ label: `   ${t.label}`, key: t.shortcut?.toUpperCase(), act: () => app.setTool(t.id) }))]);
      return items.length ? items : [{ label: 'No plugins loaded', act: () => {}, disabled: true }, { label: 'How to write one (docs/PLUGINS.md)', act: () => app.toast('Browser: window.plannr.registerPlugin({ id, name, tools, panels }). Engine: implement plannr_core::plugin::Plugin and register it in plannr-server/src/plugins.rs.', 7000) }];
    }],
    ['Help', () => [
      { label: 'Keyboard shortcuts', act: () => app.toast('Space Select · E Eraser · L Line · R Rectangle · C Circle · N Polygon · P Push/Pull · F Offset · M Move · Q Rotate · S Scale · T Tape · D Dimension · X Text · B Paint · O Orbit · H Pan · Z Zoom · ⇧Z Extents · G Group · arrows lock axis · ⌘Z undo', 8000) },
      { label: 'Mouse: middle-drag orbits, Shift+middle pans, wheel zooms', act: () => {} },
      { label: 'Connect an AI (MCP)', act: () => app.toast(`MCP endpoint: ${location.origin}/mcp — the token is printed by plannr-server at startup. Run: claude mcp add --transport http plannr ${location.origin}/mcp --header "Authorization: Bearer <token>"`, 9000) },
      { label: 'About Plannr', act: () => app.toast('Plannr · generic 3D modeller · Rust geometry engine + three.js viewport · every tool is also an MCP tool', 5000) },
    ]],
  ];
  el.innerHTML = `<div class="brand">${icon('logo')}<span>Plannr</span></div>` +
    menus.map(([name]) => `<div class="menu" data-menu="${name}"><button>${name}</button><div class="dropdown"></div></div>`).join('') +
    `<div class="spacer"></div><button class="panels-toggle" title="Show or hide the panels">${icon('panels')}</button>${app.enteredGroup ? `<span class="muted" style="margin-right:8px">editing group · Esc closes</span>` : ''}<input class="project-name" value="${esc(app.projectName)}" title="Project name"><span class="save-state">${app.saveState}</span>`;
  const closeAll = () => el.querySelectorAll('.menu.open').forEach(m => m.classList.remove('open'));
  el.querySelectorAll<HTMLElement>('.menu').forEach(m => {
    const name = m.dataset.menu!; const btn = m.querySelector('button')!; const dd = m.querySelector('.dropdown')!;
    const open = () => {
      closeAll(); m.classList.add('open');
      const items = menus.find(x => x[0] === name)![1]();
      dd.innerHTML = items.map(it => it === 'sep' ? '<hr>' : `<button ${it.disabled ? 'disabled' : ''}><span>${esc(it.label)}</span>${it.key ? `<kbd>${it.key}</kbd>` : ''}</button>`).join('');
      const acts = items.filter(x => x !== 'sep') as Exclude<MenuItem, 'sep'>[];
      dd.querySelectorAll('button').forEach((b, i) => { b.onclick = (e) => { e.stopPropagation(); closeAll(); acts[i].act(); }; });
    };
    btn.onclick = (e) => { e.stopPropagation(); m.classList.contains('open') ? closeAll() : open(); };
    btn.onmouseenter = () => { if (el.querySelector('.menu.open')) open(); };
  });
  document.addEventListener('click', closeAll);
  el.querySelector<HTMLElement>('.panels-toggle')!.onclick = () => document.getElementById('app')!.classList.toggle('tray-open');
  const nameInput = el.querySelector<HTMLInputElement>('.project-name')!;
  nameInput.onchange = () => app.rename(nameInput.value.trim() || 'Untitled');
  nameInput.onkeydown = (e) => { if (e.key === 'Enter') nameInput.blur(); e.stopPropagation(); };
}

const holdName: Record<string, string> = { Control: 'Ctrl', Alt: '⌥', Shift: '⇧', Meta: '⌘' };

export function renderToolbar(app: App, el: HTMLElement) {
  let groups: { ids: string[]; plugin?: { name: string; accent: string } }[] = [...TOOLBAR.map(ids => ({ ids })), ...getPlugins().flatMap(p => (p.toolbar ?? (p.tools?.length ? [p.tools.map(t => t.id)] : [])).map(ids => ({ ids, plugin: { name: p.name, accent: p.accent ?? '#8e44ad' } })))];
  if (app.embed) groups = groups.filter(g => !g.plugin).map(g => ({ ids: g.ids.filter(id => EMBED_TOOLS.has(id)) })).filter(g => g.ids.length);
  const top = document.getElementById('app')?.classList.contains('dock-top');
  el.innerHTML = `<button class="toolbar-grip" title="Drag to move the tools to the ${top ? 'left' : 'top'} · tap to switch">${top ? '⋮' : '⋯'}</button><div class="groups">` + groups.map(g => `<div class="group ${g.plugin ? 'plugin' : ''}" style="--n:${g.ids.length};${g.plugin ? `--plugin-accent:${g.plugin.accent}` : ''}" ${g.plugin ? `title="${esc(g.plugin.name)} plugin"` : ''}>${g.ids.map(id => {
    const t = app.tools.find(t => t.id === id);
    const hold = (Object.entries(app.modifierTools) as [string, string][]).filter(([, tid]) => tid === id).map(([k]) => holdName[k] ?? k);
    const key = t ? (t.shortcut ? (t.shortcut.trim() ? t.shortcut.toUpperCase() : 'Space') : '') : 'Shift+Z';
    const badge = t ? (t.shortcut ? (t.shortcut.trim() ? t.shortcut.toUpperCase() : '␣') : '') : '⇧Z';
    const title = `${g.plugin ? `${g.plugin.name} · ` : ''}${t ? t.label : 'Zoom Extents'}${key ? ` (${key})` : ''}${hold.length ? ` · hold ${hold.join(' or ')}` : ''}`;
    return `<button class="tool-btn ${app.tool.id === id ? 'active' : ''}" data-tool="${id}" title="${esc(title)}">${t?.icon ? `<span class="icon">${t.icon}</span>` : icon(id)}${badge ? `<kbd class="key">${esc(badge)}</kbd>` : ''}</button>`;
  }).join('')}</div>`).join('') + '</div>';
  el.querySelectorAll<HTMLElement>('[data-tool]').forEach(b => b.onclick = () => b.dataset.tool === 'zoomext' ? app.zoomExtents() : app.setTool(b.dataset.tool!));
  layoutToolbar(el);
}

/** Fit every tool group to the palette: as many columns as the width allows (on the left), as many
 *  rows as the height allows (on top); a group never mixes with another. */
export function layoutToolbar(el: HTMLElement) {
  const top = document.getElementById('app')?.classList.contains('dock-top');
  const cs = getComputedStyle(el);
  const btn = parseFloat(cs.getPropertyValue('--btn')) || 32, gap = 4;
  const fit = (avail: number) => Math.max(1, Math.floor((avail + gap) / (btn + gap)));
  if (top) {
    const avail = el.clientHeight - parseFloat(cs.paddingTop) - parseFloat(cs.paddingBottom);
    const rows = fit(avail);
    el.querySelectorAll<HTMLElement>('.group').forEach(g => g.style.setProperty('--rows', String(Math.min(rows, parseInt(g.style.getPropertyValue('--n')) || 1))));
  } else {
    const avail = el.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    const cols = fit(avail);
    el.querySelectorAll<HTMLElement>('.group').forEach(g => g.style.setProperty('--cols', String(Math.min(cols, parseInt(g.style.getPropertyValue('--n')) || 1))));
  }
}

export function renderHud(app: App, el: HTMLElement) {
  el.innerHTML = `<button data-view="iso" title="Iso">${icon('iso')}Iso</button><button data-view="top" title="Top">${icon('top')}Top</button><button data-view="front" title="Front">${icon('front')}Front</button><button data-view="right" title="Right">${icon('side')}Right</button><span class="sep"></span><button data-act="extents" title="Zoom Extents (Shift+Z)">${icon('zoomext')}</button><button data-act="prevview" title="Previous view (⌘[)" ${app.canPrevView ? '' : 'disabled'}>${icon('prevview')}</button><button data-act="nextview" title="Next view (⌘])" ${app.canNextView ? '' : 'disabled'}>${icon('nextview')}</button><span class="sep"></span><button data-act="undo" title="Undo (⌘Z)" ${app.canUndo ? '' : 'disabled'}>${icon('undo')}</button><button data-act="redo" title="Redo (⇧⌘Z)" ${app.canRedo ? '' : 'disabled'}>${icon('redo')}</button>`;
  el.querySelectorAll<HTMLElement>('[data-view]').forEach(b => b.onclick = () => app.setView(b.dataset.view as any));
  el.querySelector<HTMLElement>('[data-act="extents"]')!.onclick = () => app.zoomExtents();
  el.querySelector<HTMLElement>('[data-act="prevview"]')!.onclick = () => app.previousView(); el.querySelector<HTMLElement>('[data-act="nextview"]')!.onclick = () => app.nextView();
  el.querySelector<HTMLElement>('[data-act="undo"]')!.onclick = () => app.undo();
  el.querySelector<HTMLElement>('[data-act="redo"]')!.onclick = () => app.redo();
}

/** Contextual options for the active tool. */
export function renderToolOptions(app: App, el: HTMLElement) {
  const t = app.tool.id; let html = '';
  if (t === 'circle') html = `<label style="padding:3px 6px">Sides <input type="number" min="3" max="200" data-segs value="${app.options.circleSegments}" style="width:52px"></label>`;
  if (t === 'paint') { const m = app.doc.materials.find(m => m.id === app.options.material); html = `<span style="padding:3px 8px;display:flex;align-items:center;gap:6px"><span class="swatch-lg" style="background:${m?.color ?? '#ccc'}"></span>${esc(m?.name ?? 'Default')}</span>`; }
  if (['line', 'rectangle', 'circle', 'polygon', 'tape', 'dimension', 'text'].includes(t)) { const l = app.doc.layers.find(l => l.id === app.options.layer); html += `<span class="muted" style="padding:3px 8px">Tag: ${esc(l?.name ?? '')}</span>`; }
  el.innerHTML = html;
  el.querySelector<HTMLInputElement>('[data-segs]')?.addEventListener('change', (e) => { app.options.circleSegments = Math.max(3, Math.min(200, parseInt((e.target as HTMLInputElement).value) || 24)); });
}

export function renderWelcome(app: App, el: HTMLElement, projects: ProjectSummary[], online: boolean) {
  const date = (s: string) => new Date(s).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  el.innerHTML = `<div class="welcome"><div class="card">
    <div class="side">${icon('logo')}<h1>Plannr</h1><p>A SketchUp-style 3D modeller. Draw lines, rectangles and circles, push/pull them into solids, move, rotate, scale, offset, measure and annotate — every tool backed by a Rust engine and usable by an AI over MCP.</p>
      <p class="ver">${online ? 'Connected to plannr-server' : 'Server offline — start plannr-server on port 8795'}</p></div>
    <div class="main">
      <h2>Start</h2>
      <div class="templates">
        <div class="template" data-new="empty"><div class="t">Empty model</div><div class="d">One tag, one material, metres.</div></div>
        <div class="template" data-new="sample"><div class="t">Sample model</div><div class="d">Boxes, a circle, an L-shape, a group, a dimension and a text — to try the tools.</div></div>
        <div class="template" data-new="import"><div class="t">Import JSON…</div><div class="d">Open a Plannr document from a file.</div></div>
      </div>
      <h2>Recent projects</h2>
      <div class="recent">${projects.length ? projects.map(p => `<div class="list-row" data-open="${p.id}"><span class="name"><b>${esc(p.name)}</b><div class="meta">${p.entityCount} entities · v${p.version} · ${date(p.updatedAt)}</div></span><button class="icon-btn" data-del="${p.id}" title="Delete">${icon('trash')}</button></div>`).join('') : `<div class="empty">No projects yet.</div>`}</div>
      ${app.projectId ? `<div class="btn-row" style="margin-top:18px"><button class="btn" data-close>Back to ${esc(app.projectName)}</button></div>` : ''}
    </div></div></div>`;
  el.querySelectorAll<HTMLElement>('[data-new]').forEach(b => b.onclick = () => app.newFromTemplate(b.dataset.new as any));
  el.querySelectorAll<HTMLElement>('[data-open]').forEach(r => r.onclick = (e) => { if ((e.target as HTMLElement).closest('button')) return; app.openProject(r.dataset.open!); });
  el.querySelectorAll<HTMLElement>('[data-del]').forEach(b => b.onclick = () => app.deleteProject(b.dataset.del!));
  el.querySelector<HTMLElement>('[data-close]')?.addEventListener('click', () => app.hideWelcome());
}
