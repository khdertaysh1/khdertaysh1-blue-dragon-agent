function blueDragonPage(action, args) {
  const clip = (value, max = 40000) => String(value ?? '').slice(0, max);
  if (!window.__bbStudioConsole) {
    const rows = [];
    Object.defineProperty(window, '__bbStudioConsole', { value: rows, configurable: true });
    for (const level of ['log', 'info', 'warn', 'error']) {
      const original = console[level];
      console[level] = function (...values) {
        rows.push({level, text: values.map(x => { try { return typeof x === 'string' ? x : JSON.stringify(x); } catch { return String(x); } }).join(' ').slice(0, 4000)});
        if (rows.length > 200) rows.shift();
        return original.apply(this, values);
      };
    }
  }
  const selectorFor = el => {
    if (el.id) return '#' + CSS.escape(el.id);
    const parts = [];
    for (let node = el; node && node.nodeType === 1 && parts.length < 12; node = node.parentElement) {
      const siblings = node.parentElement ? [...node.parentElement.children].filter(n => n.tagName === node.tagName) : [];
      parts.unshift(node.tagName.toLowerCase() + (siblings.length > 1 ? `:nth-of-type(${siblings.indexOf(node)+1})` : ''));
      if (node === document.body) break;
    }
    return parts.join(' > ');
  };
  if (action === 'snapshot') {
    return {untrusted_web_content: true, url: location.href, title: document.title, ready: document.readyState,
      text: clip(document.body?.innerText),
      elements: [...document.querySelectorAll('a,button,input,textarea,select,[role="button"]')].filter(e => e.getClientRects().length).slice(0, 100).map(e => ({
        selector: selectorFor(e), tag: e.tagName.toLowerCase(), type: e.type || '',
        label: clip(e.getAttribute('aria-label') || e.innerText || e.placeholder || e.name, 200),
        href: e.tagName === 'A' ? e.href : undefined, disabled: !!e.disabled
      })), scripts: [...document.scripts].map(e => e.src).filter(Boolean).slice(0, 100)};
  }
  if (action === 'console') return {untrusted_web_content: true, entries: window.__bbStudioConsole.slice(-100), note: 'Extension console capture begins after first interaction; early page logs may be absent.'};
  if (action === 'evaluate') {
    if (typeof args.expression !== 'string' || args.expression.length > 16000) throw new Error('Invalid expression');
    // Runs in MAIN/page world, never the privileged extension world.
    return (0, eval)(args.expression);
  }
  const nodes = document.querySelectorAll(args.selector);
  if (nodes.length !== 1) throw new Error('Selector must match exactly one element; inspect a fresh snapshot');
  const el = nodes[0];
  if (!el.getClientRects().length || el.disabled) throw new Error('Element is hidden or disabled');
  if (action === 'click') { el.scrollIntoView({block: 'center'}); el.click(); return {clicked: true}; }
  if (action === 'fill') {
    if (!['INPUT','TEXTAREA','SELECT'].includes(el.tagName) || ['file','checkbox','radio','submit','button'].includes(el.type)) throw new Error('Use a text/select control');
    if (typeof args.value !== 'string' || args.value.length > 16000) throw new Error('Invalid value');
    const prototype = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(el, args.value);
    el.dispatchEvent(new Event('input', {bubbles:true})); el.dispatchEvent(new Event('change', {bubbles:true}));
    return {filled: true};
  }
  throw new Error('Unsupported page action');
}
