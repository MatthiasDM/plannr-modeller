// In-app replacements for prompt() / confirm() (blocked in embedded browsers) and instant tooltips.

function openDialog(build: (body: HTMLElement, close: (v: any) => void) => void): Promise<any> {
  return new Promise(resolve => {
    const host = document.createElement('div'); host.className = 'dlg-host';
    const card = document.createElement('div'); card.className = 'dlg'; host.appendChild(card);
    const close = (v: any) => { host.remove(); window.removeEventListener('keydown', onKey, true); resolve(v); };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); close(null); } };
    window.addEventListener('keydown', onKey, true);
    host.addEventListener('pointerdown', e => { if (e.target === host) close(null); });
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
