// In-app replacements for prompt() / confirm() (blocked in embedded browsers) and instant tooltips.

function openDialog(build: (body: HTMLElement, close: (v: any) => void) => void, opts: { wide?: boolean; backdropCloses?: boolean } = {}): Promise<any> {
  return new Promise(resolve => {
    const host = document.createElement('div'); host.className = 'dlg-host';
    const card = document.createElement('div'); card.className = `dlg${opts.wide ? ' wide' : ''}`; host.appendChild(card);
    const close = (v: any) => { host.remove(); window.removeEventListener('keydown', onKey, true); resolve(v); };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); close(null); } };
    window.addEventListener('keydown', onKey, true);
    if (opts.backdropCloses !== false) host.addEventListener('pointerdown', e => { if (e.target === host) close(null); });
    build(card, close);
    document.body.appendChild(host);
  });
}

/** Ask for a line of text. Resolves null when cancelled. */
export function promptText(label: string, value = '', okLabel = 'OK'): Promise<string | null> {
  return openDialog((card, close) => {
    card.innerHTML = `<div class="dlg-title"></div><input class="dlg-input" type="text" spellcheck="false"><div class="dlg-actions"><button class="btn" data-cancel>Cancel</button><button class="btn primary" data-ok></button></div>`;
    card.querySelector('.dlg-title')!.textContent = label;
    const input = card.querySelector<HTMLInputElement>('.dlg-input')!; input.value = value;
    card.querySelector('[data-ok]')!.textContent = okLabel;
    const ok = () => close(input.value);
    input.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') ok(); if (e.key === 'Escape') close(null); });
    card.querySelector<HTMLElement>('[data-ok]')!.onclick = ok; card.querySelector<HTMLElement>('[data-cancel]')!.onclick = () => close(null);
    setTimeout(() => { input.focus(); input.select(); }, 0);
  });
}

/** Yes / no question. */
export function confirmDialog(text: string, okLabel = 'OK', danger = false): Promise<boolean> {
  return openDialog((card, close) => {
    card.innerHTML = `<div class="dlg-text"></div><div class="dlg-actions"><button class="btn" data-cancel>Cancel</button><button class="btn primary ${danger ? 'danger' : ''}" data-ok></button></div>`;
    card.querySelector('.dlg-text')!.textContent = text; card.querySelector('[data-ok]')!.textContent = okLabel;
    card.querySelector<HTMLElement>('[data-ok]')!.onclick = () => close(true); card.querySelector<HTMLElement>('[data-cancel]')!.onclick = () => close(false);
    setTimeout(() => card.querySelector<HTMLElement>('[data-ok]')!.focus(), 0);
  }).then(v => !!v);
}

const escHtml = (s: string) => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!));

/**
 * "Connect an AI (MCP)": a dialog that stays open until closed. Shows the endpoint, where the token
 * comes from, lets the user paste it and test it against `/mcp`, and gives the Claude Code command
 * and a JSON config for other MCP clients, each with a copy button, filled in with the token.
 * The browser never receives the token from the server (that would hand it to any page); the user
 * copies it from the terminal that runs plannr-server.
 */
export function mcpDialog(notify: (text: string) => void): Promise<void> {
  const endpoint = `${location.origin}/mcp`;
  return openDialog((card, close) => {
    const PH = '<token>';
    const cmd = (t: string) => `claude mcp add --transport http plannr ${endpoint} --header "Authorization: Bearer ${t}"`;
    const cfg = (t: string) => JSON.stringify({ mcpServers: { plannr: { type: 'http', url: endpoint, headers: { Authorization: `Bearer ${t}` } } } }, null, 2);
    card.innerHTML = `
      <div class="dlg-head"><div class="dlg-title">Connect an AI over MCP</div><button class="icon-btn dlg-x" title="Close" data-close>✕</button></div>
      <div class="dlg-text">Plannr speaks the Model Context Protocol: every tool, the camera and screenshots are available to an assistant such as Claude Code. Four steps, all on this machine.</div>
      <ol class="dlg-steps">
        <li><b>Endpoint</b><div class="dlg-code"><code>${escHtml(endpoint)}</code><button class="btn" data-copy="endpoint">Copy</button></div></li>
        <li><b>Token</b><div class="dlg-note" data-token-note>Reading the token of the running server…</div>
          <div class="dlg-code"><input class="dlg-input" type="text" spellcheck="false" autocomplete="off" placeholder="paste the token" data-token><button class="btn" data-test>Test</button></div>
          <div class="dlg-status" data-status></div></li>
        <li><b>Claude Code</b><div class="dlg-note">Run once in a terminal, then start <code>claude</code> and ask it to model.</div><div class="dlg-code"><code data-cmd>${escHtml(cmd(PH))}</code><button class="btn" data-copy="cmd">Copy</button></div></li>
        <li><b>Other MCP clients</b><div class="dlg-note">Claude Desktop, Cursor, … take this in their MCP settings (Streamable HTTP, bearer header).</div><div class="dlg-code"><code class="block" data-cfg>${escHtml(cfg(PH))}</code><button class="btn" data-copy="cfg">Copy</button></div></li>
      </ol>
      <div class="dlg-note">Keep this project open in a browser: <code>screenshot</code> and the camera tools act through it. Tell the assistant which project to use (<code>use_project</code>) or name it on each call.</div>
      <div class="dlg-actions"><button class="btn primary" data-close>Close</button></div>`;
    const tokenInput = card.querySelector<HTMLInputElement>('[data-token]')!;
    const note = card.querySelector<HTMLElement>('[data-token-note]')!;
    const MANUAL = 'The terminal that runs <code>plannr-server</code> prints it at startup on the line <code>MCP token: …</code>. Set <code>PLANNR_TOKEN=…</code> before starting the server to choose a fixed one. Paste it here to fill in the commands below and to test it; it stays in this window.';
    const status = card.querySelector<HTMLElement>('[data-status]')!;
    const token = () => tokenInput.value.trim() || PH;
    const texts: Record<string, () => string> = { endpoint: () => endpoint, cmd: () => cmd(token()), cfg: () => cfg(token()) };
    const refill = () => { card.querySelector('[data-cmd]')!.textContent = cmd(token()); card.querySelector('[data-cfg]')!.textContent = cfg(token()); };
    tokenInput.addEventListener('input', refill);
    tokenInput.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') test(); if (e.key === 'Escape') close(null); });
    card.querySelectorAll<HTMLElement>('[data-copy]').forEach(b => b.onclick = async () => {
      const t = texts[b.dataset.copy!]();
      try { await navigator.clipboard.writeText(t); b.textContent = 'Copied'; setTimeout(() => { b.textContent = 'Copy'; }, 1400); }
      catch { notify('Clipboard blocked — select the text and copy it by hand'); }
    });
    const setStatus = (kind: 'ok' | 'bad' | 'busy' | '', text: string) => { status.className = `dlg-status ${kind}`; status.textContent = text; };
    const test = async () => {
      const t = tokenInput.value.trim(); if (!t) { setStatus('bad', 'Paste the token first.'); tokenInput.focus(); return; }
      setStatus('busy', 'Testing…');
      try {
        const res = await fetch(endpoint, { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${t}` }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list' }) });
        if (res.status === 404) { setStatus('bad', 'This instance has no MCP endpoint (public mode) — run plannr-server on your own machine.'); return; }
        const body = await res.json().catch(() => ({}));
        if (res.status === 401 || body?.error?.code === -32001) { setStatus('bad', 'Rejected: that is not the token of the running server. Check the terminal line “MCP token: …”.'); return; }
        if (!res.ok || !body?.result) { setStatus('bad', `Unexpected answer (${res.status}): ${body?.error?.message ?? 'see the server log'}`); return; }
        const n = body.result.tools?.length ?? 0;
        setStatus('ok', `Connected — the server accepts this token and offers ${n} tools. Copy the command below.`);
      } catch (e: any) { setStatus('bad', `Cannot reach ${endpoint}: ${e?.message ?? e}`); }
    };
    card.querySelector<HTMLElement>('[data-test]')!.onclick = test;
    // The server hands its token only to a browser on the same machine, same origin, private instance.
    fetch('/api/mcp-token', { credentials: 'same-origin' }).then(async r => {
      const body = await r.json().catch(() => ({}));
      if (r.ok && body?.token) {
        tokenInput.value = body.token; refill();
        note.innerHTML = 'This is the token of the running server (it was generated at startup, or set with <code>PLANNR_TOKEN</code>); the commands below carry it already. It is shown because this browser runs on the machine that runs <code>plannr-server</code>.';
        setStatus('ok', 'Token read from the running server.');
      } else {
        note.innerHTML = (r.status === 403 ? 'Plannr shows the token only when you open it on the machine that runs the server. ' : '') + MANUAL;
      }
    }).catch(() => { note.innerHTML = MANUAL; });
    card.querySelectorAll<HTMLElement>('[data-close]').forEach(b => b.onclick = () => close(null));
    setTimeout(() => tokenInput.focus(), 0);
  }, { wide: true, backdropCloses: false }).then(() => undefined);
}

/** Instant tooltips: any element with a `title` shows a styled tip after 120 ms (the native one is moved to data-tip). */
export function installTooltips() {
  const tip = document.createElement('div'); tip.className = 'tip'; document.body.appendChild(tip);
  let timer: any = null; let current: HTMLElement | null = null;
  const hide = () => { clearTimeout(timer); tip.classList.remove('show'); current = null; };
  document.addEventListener('mouseover', e => {
    const el = (e.target as HTMLElement).closest<HTMLElement>('[title], [data-tip]'); if (!el || el === current) return;
    if (el.hasAttribute('title')) { el.dataset.tip = el.getAttribute('title') ?? ''; el.removeAttribute('title'); }
    const text = el.dataset.tip; hide(); if (!text) return; current = el;
    timer = setTimeout(() => {
      tip.textContent = text; const r = el.getBoundingClientRect(); tip.classList.add('show');
      const w = tip.offsetWidth, h = tip.offsetHeight; let x = r.left + r.width / 2 - w / 2, y = r.bottom + 8;
      if (r.left < 90) { x = r.right + 8; y = r.top + r.height / 2 - h / 2; } // tool palette: show to the right
      x = Math.max(6, Math.min(window.innerWidth - w - 6, x)); if (y + h > window.innerHeight - 6) y = r.top - h - 8;
      tip.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
    }, 120);
  });
  document.addEventListener('mouseout', e => { const el = (e.target as HTMLElement).closest('[data-tip]'); if (el && el === current) hide(); });
  document.addEventListener('pointerdown', hide, true);
}

/** Where packaged plugins are published (GitHub releases of the public repository). */
export const PLUGIN_RELEASES = 'https://github.com/MatthiasDM/plannr-plugins/releases';

interface ManagedPlugin { id: string; name: string; version?: string; description?: string; homepage?: string; enabled?: boolean; source?: 'builtIn' | 'installed'; engine?: boolean; browserOnly?: boolean; suggests?: string[]; ops?: unknown[]; exports?: { format: string }[] }

/**
 * Plugins → Manage plugins…: every plugin with an on/off switch, Install from file (a `.plannr-plugin`
 * package, also by dropping it on the dialog), Remove for installed ones. Changes apply after a reload,
 * which the dialog offers (and does on close). On a public instance the server asks for its admin token.
 */
export function pluginsDialog(owner: string, notify: (text: string) => void): Promise<void> {
  let changed = false;
  const tokenKey = 'plannr.adminToken';
  const token = () => { try { return sessionStorage.getItem(tokenKey) ?? ''; } catch { return ''; } };
  const headers = (extra: Record<string, string> = {}) => { const h: Record<string, string> = { 'X-Plannr-Owner': owner, ...extra }; const t = token(); if (t) h.Authorization = `Bearer ${t}`; return h; };
  return openDialog((card, close) => {
    const done = () => { close(null); if (changed) location.reload(); };
    card.classList.add('plugins-dlg');
    card.innerHTML = `
      <div class="dlg-head"><div class="dlg-title">Plugins</div><button class="icon-btn dlg-x" title="Close" data-close>✕</button></div>
      <div class="dlg-note">Switch plugins on and off, or install one from a <code>.plannr-plugin</code> file — download them from <a href="${PLUGIN_RELEASES}" target="_blank" rel="noopener">plannr-plugins on GitHub</a>. A plugin that is off keeps its objects in your models; they show again when it is back on.</div>
      <div class="plg-list" data-list><div class="dlg-note">Loading…</div></div>
      <div class="dlg-status" data-status></div>
      <div class="plg-token" data-token hidden><div class="dlg-note">This is a public instance: managing its plugins needs the server's admin token (<code>PLANNR_TOKEN</code>).</div><div class="dlg-code"><input class="dlg-input" type="password" placeholder="admin token" data-token-input><button class="btn" data-token-ok>Use</button></div></div>
      <div class="plg-reload" data-reload hidden>Changes apply when the app reloads. <button class="btn primary" data-reload-now>Reload now</button></div>
      <div class="dlg-actions"><input type="file" accept=".plannr-plugin,.zip" data-file hidden><button class="btn" data-install>Install from file…</button><span style="flex:1"></span><button class="btn primary" data-done>Done</button></div>`;
    const list = card.querySelector<HTMLElement>('[data-list]')!;
    const status = card.querySelector<HTMLElement>('[data-status]')!;
    const setStatus = (kind: 'ok' | 'bad' | 'busy' | '', text: string) => { status.className = `dlg-status ${kind}`; status.textContent = text; };
    const markChanged = () => { changed = true; card.querySelector<HTMLElement>('[data-reload]')!.hidden = false; };
    let retry: (() => void) | null = null;
    const needToken = (again: () => void) => { retry = again; card.querySelector<HTMLElement>('[data-token]')!.hidden = false; card.querySelector<HTMLInputElement>('[data-token-input]')!.focus(); };
    async function send(method: string, url: string, body?: BodyInit, json = false): Promise<any> {
      const res = await fetch(url, { method, headers: headers(json ? { 'Content-Type': 'application/json' } : {}), body });
      const data = await res.json().catch(() => ({}));
      if (res.status === 403 && data?.needsToken) { const e: any = new Error(data.error); e.needsToken = true; throw e; }
      if (!res.ok) throw new Error(data?.error ?? `${res.status} ${res.statusText}`);
      return data;
    }
    async function render() {
      let all: ManagedPlugin[];
      try { all = await (await fetch('/api/plugins', { headers: headers() })).json(); } catch (e: any) { list.innerHTML = `<div class="dlg-status bad">${escHtml(e.message ?? String(e))}</div>`; return; }
      const names = new Map(all.map(p => [p.id, p.name || p.id]));
      all.sort((a, b) => (a.source === 'installed' ? 0 : 1) - (b.source === 'installed' ? 0 : 1) || (a.name || a.id).localeCompare(b.name || b.id));
      list.innerHTML = all.map(p => {
        const on = p.enabled !== false; const kind = p.source === 'installed' ? 'Installed' : 'Built in';
        const nOps = (p.ops ?? []).length; const ex = (p.exports ?? []).map(e => e.format.toUpperCase());
        const what = p.engine === false ? 'browser only' : [nOps ? `${nOps} tools` : '', ex.length ? `writes ${ex.join(', ')}` : ''].filter(Boolean).join(' · ') || 'engine';
        const missing = (p.suggests ?? []).filter(s => !names.has(s));
        return `<div class="plg-row ${on ? '' : 'off'}">
          <label class="plg-switch" title="${on ? 'Switch off' : 'Switch on'}"><input type="checkbox" data-toggle="${escHtml(p.id)}" ${on ? 'checked' : ''}><span></span></label>
          <div class="plg-main"><div class="plg-name">${escHtml(p.name || p.id)} <span class="plg-meta">${escHtml(p.id)}${p.version ? ` · ${escHtml(p.version)}` : ''} · ${kind} · ${what}</span></div>
            ${p.description ? `<div class="plg-desc">${escHtml(p.description)}</div>` : ''}
            ${missing.length ? `<div class="plg-desc">Works with ${missing.map(escHtml).join(', ')} (not installed)</div>` : ''}
            ${p.homepage ? `<a class="plg-link" href="${escHtml(p.homepage)}" target="_blank" rel="noopener">Documentation</a>` : ''}</div>
          ${p.source === 'installed' ? `<button class="btn" data-remove="${escHtml(p.id)}" title="Uninstall">Remove</button>` : ''}</div>`;
      }).join('') || '<div class="dlg-note">No plugins.</div>';
      list.querySelectorAll<HTMLInputElement>('[data-toggle]').forEach(cb => cb.onchange = () => toggle(cb.dataset.toggle!, cb.checked));
      list.querySelectorAll<HTMLElement>('[data-remove]').forEach(b => b.onclick = () => remove(b.dataset.remove!));
    }
    async function toggle(id: string, on: boolean) {
      try { await send('POST', `/api/plugins/${encodeURIComponent(id)}/enabled`, JSON.stringify({ enabled: on }), true); setStatus('ok', `${id} switched ${on ? 'on' : 'off'}.`); markChanged(); }
      catch (e: any) { if (e.needsToken) needToken(() => toggle(id, on)); else setStatus('bad', e.message); }
      render();
    }
    async function remove(id: string) {
      if (!(await confirmDialog(`Remove the ${id} plugin? Its objects stay in your models and show again if you install it later.`, 'Remove', true))) return;
      try { await send('DELETE', `/api/plugins/${encodeURIComponent(id)}`); setStatus('ok', `${id} removed.`); markChanged(); }
      catch (e: any) { if (e.needsToken) needToken(() => remove(id)); else setStatus('bad', e.message); }
      render();
    }
    async function install(file: File) {
      setStatus('busy', `Installing ${file.name}…`);
      try { const r = await send('POST', '/api/plugins/install', await file.arrayBuffer()); setStatus('ok', `Installed ${r.installed.name} ${r.installed.version}.`); markChanged(); notify(`Plugin ${r.installed.name} installed`); }
      catch (e: any) { if (e.needsToken) needToken(() => install(file)); else setStatus('bad', `${file.name}: ${e.message}`); }
      render();
    }
    const fileInput = card.querySelector<HTMLInputElement>('[data-file]')!;
    fileInput.onchange = () => { const f = fileInput.files?.[0]; if (f) install(f); fileInput.value = ''; };
    card.querySelector<HTMLElement>('[data-install]')!.onclick = () => fileInput.click();
    card.addEventListener('dragover', e => { e.preventDefault(); card.classList.add('drop'); });
    card.addEventListener('dragleave', () => card.classList.remove('drop'));
    card.addEventListener('drop', e => { e.preventDefault(); card.classList.remove('drop'); const f = e.dataTransfer?.files?.[0]; if (f) install(f); });
    card.querySelector<HTMLElement>('[data-token-ok]')!.onclick = () => {
      const v = card.querySelector<HTMLInputElement>('[data-token-input]')!.value.trim(); if (!v) return;
      try { sessionStorage.setItem(tokenKey, v); } catch { /* storage blocked */ }
      card.querySelector<HTMLElement>('[data-token]')!.hidden = true; const r = retry; retry = null; r?.();
    };
    card.querySelector<HTMLInputElement>('[data-token-input]')!.addEventListener('keydown', e => e.stopPropagation());
    card.querySelector<HTMLElement>('[data-reload-now]')!.onclick = () => location.reload();
    card.querySelectorAll<HTMLElement>('[data-close], [data-done]').forEach(b => b.onclick = done);
    render();
  }, { wide: true });
}
