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
  // One shared budget across all selects, not 100 options per element unbounded.
  let optionBudget = 12000;
  const optionSummary = el => {
    const options = [];
    for (const o of el.options) {
      if (options.length >= 100) break;
      const value = String(o.value ?? ''), label = String(o.label ?? '');
      const row = {value: clip(value, 200), value_truncated: value.length > 200,
        label: clip(label, 200), label_truncated: label.length > 200,
        disabled: !!o.disabled || !!o.parentElement?.disabled, selected: !!o.selected};
      const cost = JSON.stringify(row).length;
      if (cost > optionBudget) break;
      optionBudget -= cost; options.push(row);
    }
    return {options, options_omitted: Math.max(0, el.options.length - options.length),
      option_value_note: 'Truncated values are previews, not usable exact selection values.'};
  };
  if (action === 'snapshot') {
    return {untrusted_web_content: true, url: location.href, title: document.title, ready: document.readyState,
      text: clip(document.body?.innerText),
      forms: [...document.forms].slice(0, 20).map(f => ({action: f.action, method: f.method,
        fields: [...f.elements].slice(0, 60).map(e => ({tag: e.tagName.toLowerCase(), type: e.type || '', name: clip(e.name, 100), selector: selectorFor(e), required: !!e.required, minlength: e.minLength, maxlength: e.maxLength, pattern: clip(e.pattern, 100), autocomplete: clip(e.autocomplete, 100)}))})),
      inline_scripts: [...document.scripts].filter(e => !e.src).slice(0, 6).map(e => clip(e.textContent, 4000)),
      elements: [...document.querySelectorAll('a,button,input,textarea,select,[role="button"]')].filter(e => e.getClientRects().length).slice(0, 100).map(e => ({
        selector: selectorFor(e), tag: e.tagName.toLowerCase(), type: e.type || '',
        label: clip(e.getAttribute('aria-label') || e.innerText || e.placeholder || e.name, 200),
        href: e.tagName === 'A' ? e.href : undefined, disabled: !!e.disabled, checked: ['checkbox','radio'].includes(e.type) ? !!e.checked : undefined,
        ...(e.tagName === 'SELECT' ? optionSummary(e) : {})
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
  if (!el.getClientRects().length || el.disabled || el.matches(':disabled')) throw new Error('Element is hidden or disabled');
  if (action === 'set_checked') {
    if (el.tagName !== 'INPUT' || !['checkbox','radio'].includes(el.type) || typeof args.checked !== 'boolean') throw new Error('Expected checkbox/radio and boolean checked');
    if (el.type === 'radio' && !args.checked) throw new Error('Select another radio option instead of clearing a radio');
    const changed = el.checked !== args.checked;
    if (changed) { el.scrollIntoView({block:'center'}); el.click(); }
    return {changed, checked: !!el.checked, matched: el.checked === args.checked, note: 'Observed DOM state only; click handlers may submit forms, make network requests or change state later. No retry performed.'};
  }
  if (action === 'select') {
    if (el.tagName !== 'SELECT' || el.multiple || typeof args.value !== 'string' || args.value.length > 16000) throw new Error('Expected single select and exact option value');
    const options = [...el.options].filter(o => o.value === args.value);
    if (options.length !== 1 || options[0].disabled || options[0].parentElement?.disabled) throw new Error('Option must exist uniquely and be enabled');
    const changed = el.value !== args.value;
    if (changed) {
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set.call(el, args.value);
      el.dispatchEvent(new Event('input', {bubbles:true})); el.dispatchEvent(new Event('change', {bubbles:true}));
    }
    return {changed, value: clip(el.value, 16000), matched: el.value === args.value, note: 'Observed DOM state only; change handlers may submit forms or make network requests. No retry performed.'};
  }
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
