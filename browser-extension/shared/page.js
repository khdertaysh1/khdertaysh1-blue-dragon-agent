function blueDragonPage(action, args, transportEnvelope = false) {
  const run = () => {
  const outerDocument = globalThis.document;
  let document = outerDocument;
  if (args.frame_selector !== undefined) {
    if (typeof args.frame_selector !== 'string' || args.frame_selector.length > 500) throw new Error('Invalid frame selector');
    if (!['snapshot','wait_for','click','fill','select','set_checked','scroll'].includes(action)) throw new Error('Action does not support frame targeting');
    const frames = outerDocument.querySelectorAll(args.frame_selector);
    if (frames.length !== 1 || frames[0].tagName !== 'IFRAME') throw new Error('Frame selector must match exactly one iframe');
    try { document = frames[0].contentDocument; if (!document || !document.location.href) throw new Error('inaccessible'); }
    catch { throw new Error('Cross-origin or unavailable iframe; no action performed'); }
  }
  const window = document.defaultView || globalThis.window;
  const location = document.location || globalThis.location;
  const clip = (value, max = 40000) => String(value ?? '').slice(0, max);
  if (!window.__bbStudioConsole) {
    const rows = [];
    Object.defineProperty(window, '__bbStudioConsole', { value: rows, configurable: true });
    window.addEventListener?.('error', event => {
      if (typeof event.message !== 'string') return;
      rows.push({level:'error', kind:'uncaught_exception', text:clip(event.message,1000),
        url:clip(event.filename,2000), line:event.lineno || null, column:event.colno || null});
      if (rows.length > 200) rows.shift();
    });
    for (const level of ['log', 'info', 'warn', 'error']) {
      const pageConsole = window.console || globalThis.console;
      const original = pageConsole[level];
      pageConsole[level] = function (...values) {
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
  if (action === 'wait_for') {
    if (typeof args.selector !== 'string' || !args.selector || args.selector.length > 500) throw new Error('Readiness requires a bounded selector');
    const timeout = args.timeout_ms === undefined ? 5000 : args.timeout_ms;
    if (!Number.isInteger(timeout) || timeout < 100 || timeout > 10000) throw new Error('Readiness timeout must be 100..10000ms');
    if (args.text !== undefined && (typeof args.text !== 'string' || args.text.length > 500)) throw new Error('Invalid expected text');
    return (async () => {
      const started = Date.now(); let stable = 0, previous;
      while (Date.now() - started < timeout) {
        const nodes = document.querySelectorAll(args.selector);
        const el = nodes.length === 1 ? nodes[0] : null;
        const text = clip(el?.innerText || el?.textContent, 4000);
        const matched = document.readyState !== 'loading' && el && el.getClientRects().length && (args.text === undefined || text.includes(args.text));
        stable = matched ? (text === previous ? stable + 1 : 1) : 0; previous = text;
        if (stable >= 3) return {matched:true, url:location.href, criterion:'one visible selector, optional expected text, three equal 100ms samples after DOM loading', elapsed_ms:Date.now()-started, react_complete_proven:false};
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      return {matched:false, url:location.href, elapsed_ms:Date.now()-started, react_complete_proven:false, note:'Readiness criterion not observed before deadline; no framework completion or absence claim.'};
    })();
  }
  if (action === 'scroll') {
    const dx = args.delta_x === undefined ? 0 : args.delta_x;
    const dy = args.delta_y === undefined ? 0 : args.delta_y;
    if (![dx,dy].every(n => Number.isInteger(n) && Math.abs(n) <= 2000) || (dx === 0 && dy === 0)) throw new Error('Scroll requires nonzero integer deltas within -2000..2000 CSS pixels');
    let el = document.scrollingElement;
    if (args.selector !== undefined) {
      if (typeof args.selector !== 'string' || !args.selector || args.selector.length > 500) throw new Error('Invalid scroll selector');
      const nodes = document.querySelectorAll(args.selector);
      if (nodes.length !== 1) throw new Error('Scroll selector must match exactly one element');
      el = nodes[0];
    }
    if (!el || !el.getClientRects().length || typeof el.scrollTo !== 'function') throw new Error('Scroll target unavailable or hidden');
    const style = window.getComputedStyle(el);
    if (style.visibility === 'hidden' || style.visibility === 'collapse' || style.display === 'none') throw new Error('Scroll target is not CSS-visible');
    const before = {x:el.scrollLeft,y:el.scrollTop};
    // Exactly one scroll operation; no scrollIntoView (which could move ancestors).
    el.scrollTo({left:before.x+dx,top:before.y+dy,behavior:'instant'});
    const after = {x:el.scrollLeft,y:el.scrollTop};
    return {url:location.href,target:args.selector === undefined ? 'document' : 'selector',before,after,
      requested_delta:{x:dx,y:dy},observed_delta:{x:after.x-before.x,y:after.y-before.y},
      movement_observed:before.x!==after.x || before.y!==after.y,loading_complete_proven:false,
      automatic_retry:false,note:'Current offsets only; clamping, scroll snapping or page handlers may alter movement. Inspect a fresh snapshot/readiness condition; no completion or end-of-content claim.'};
  }
  if (action === 'snapshot') {
    const external = Array.from(document.scripts || []).filter(script => script.src);
    const scriptTags = external.slice(0, args.compact === true ? 12 : 100).map(script => ({
      src: clip(script.src, 2000), src_truncated: script.src.length > 2000,
      type: clip(script.type, 80), async: !!script.async, defer: !!script.defer
    }));
    const scriptMetadata = external.length ? {script_tags: scriptTags,
      script_tags_omitted: external.length - scriptTags.length} : {};
    const h1 = [...document.querySelectorAll('h1')].slice(0,4).map(e=>clip(e.innerText || e.textContent,500));
    if (args.compact === true) {
      const selector = args.selector || 'a,button,input,textarea,select,summary,[role="button"]';
      const nodes = [...document.querySelectorAll(selector)];
      return {...scriptMetadata,untrusted_web_content:true,url:location.href,title:clip(document.title,1000),h1,
        scripts:Array.from(document.scripts || []).filter(e=>e.src).slice(0,12).map(e=>e.src),
        ready:document.readyState,observation_kind:'current_page_read',compact:true,
        elements:nodes.slice(0,12).map(e=>({selector:selectorFor(e),tag:e.tagName.toLowerCase(),text:clip(e.innerText || e.textContent,500),href:e.tagName==='A'?e.href:undefined,
          ...(e.tagName==='SELECT'?optionSummary(e):{})})),
        elements_omitted:Math.max(0,nodes.length-12),text:clip(document.body?.innerText,2000),
        limits:'Bounded current DOM read, not browser readiness alone or proof of server-side effects.'};
    }
    return {...scriptMetadata,untrusted_web_content: true, h1, url: location.href, title: document.title, ready: document.readyState,
      text: clip(document.body?.innerText),
      frames: [...document.querySelectorAll('iframe')].slice(0,20).map(f => ({selector:selectorFor(f),url:f.src||'',same_origin_accessible:(()=>{try{return !!f.contentDocument;}catch{return false;}})()})),
      forms_total: document.forms.length, forms_omitted: Math.max(0, document.forms.length - 20), forms_truncated: document.forms.length > 20,
      forms: Array.from({length: Math.min(document.forms.length, 20)}, (_, i) => document.forms[i]).map(f => ({action: f.action, method: f.method,
        fields_total: f.elements.length, fields_omitted: Math.max(0, f.elements.length - 60), fields_truncated: f.elements.length > 60,
        fields: Array.from({length: Math.min(f.elements.length, 60)}, (_, i) => f.elements[i]).map(e => ({tag: e.tagName.toLowerCase(), type: e.type || '', name: clip(e.name, 100), name_truncated: String(e.name ?? '').length > 100, selector: selectorFor(e), required: !!e.required, minlength: e.minLength, maxlength: e.maxLength, pattern: clip(e.pattern, 100), autocomplete: clip(e.autocomplete, 100)}))})),
      inline_scripts: [...document.scripts].filter(e => !e.src).slice(0, 6).map(e => clip(e.textContent, 4000)),
      elements: [...document.querySelectorAll('a,button,input,textarea,select,summary,[role="button"]')].filter(e => e.getClientRects().length).slice(0, 100).map(e => ({
        selector: selectorFor(e), tag: e.tagName.toLowerCase(), type: e.type || '',
        label: clip(e.getAttribute('aria-label') || e.innerText || e.placeholder || e.name, 200),
        href: e.tagName === 'A' ? e.href : undefined, disabled: !!e.disabled, checked: ['checkbox','radio'].includes(e.type) ? !!e.checked : undefined,
        ...(e.tagName === 'SELECT' ? optionSummary(e) : {})
      })), scripts: [...document.scripts].map(e => e.src).filter(Boolean).slice(0, 100)};
  }
  if (action === 'console') return {url: location.href, untrusted_web_content: true, entries: window.__bbStudioConsole.slice(-100), note: 'Extension console capture begins after first interaction; early page logs may be absent.'};
  if (action === 'evaluate') {
    if (typeof args.expression !== 'string' || args.expression.length > 16000) throw new Error('Invalid expression');
    // Runs in MAIN/page world, never the privileged extension world.
    return (0, eval)(args.expression);
  }
  // These receipts are emitted only before invoking the requested DOM action.
  // Exceptions during a setter, event dispatch, click or observation stay uncertain.
  const formTarget = action === 'click' || action === 'fill';
  const rejected = reason => ({precondition_failed:reason});
  if (formTarget && (typeof args.selector !== 'string' || !args.selector.trim())) return rejected('missing_selector');
  let nodes;
  try { nodes = document.querySelectorAll(args.selector); }
  catch (err) {
    if (err.name === 'SyntaxError') return rejected('invalid_selector');
    throw err;
  }
  if (nodes.length !== 1) {
    if (formTarget) return rejected(nodes.length ? 'multiple_matches' : 'no_match');
    throw new Error('Selector must match exactly one element; inspect a fresh snapshot');
  }
  const el = nodes[0];
  if (!el.getClientRects().length || el.disabled || el.matches(':disabled')) {
    if (formTarget) return rejected('hidden_or_disabled');
    throw new Error('Element is hidden or disabled');
  }
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
  if (action === 'click') {
    el.scrollIntoView({block: 'center'});
    // Sample only bounded public UI state. Never read form values.
    const sample = () => ({
      target_connected: el.isConnected,
      target_expanded: el.getAttribute('aria-expanded'),
      target_pressed: el.getAttribute('aria-pressed'),
      heading: clip(document.querySelector('h1')?.textContent, 300),
      url: location.href
    });
    const before = sample();
    el.click();
    const differences = state => Object.keys(before).filter(key => before[key] !== state[key]);
    const immediate = differences(sample());
    const receipt = {clicked:true, changed_fields:immediate, observation:'immediate_dom_sample',
      effect:immediate.length ? 'change_observed' : 'unverified', automatic_retry:false};
    if (args.observe_after_click !== true) return receipt;
    return new Promise(resolve => setTimeout(resolve, 300)).then(() => {
      const delayed = differences(sample());
      return {...receipt, observation:'immediate_and_delayed_dom_samples', sample_delay_ms:300,
        delayed_changed_fields:delayed,
        effect:immediate.length || delayed.length ? 'change_observed' : 'unverified'};
    });
  }
  if (action === 'fill') {
    if (!['INPUT','TEXTAREA','SELECT'].includes(el.tagName) || ['file','checkbox','radio','submit','button'].includes(el.type)) return rejected('unsupported_control');
    if (typeof args.value !== 'string' || args.value.length > 16000) return rejected('invalid_value');
    if (el.readOnly) return rejected('read_only');
    const prototype = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(el, args.value);
    el.dispatchEvent(new Event('input', {bubbles:true})); el.dispatchEvent(new Event('change', {bubbles:true}));
    const matched = el.isConnected && el.value === args.value;
    return {filled: matched, matched};
  }
  throw new Error('Unsupported page action');
  };
  // Chrome can omit both InjectionResult.result and .error for a thrown page
  // function. Catch inside the injected function, including Promise rejection.
  // Keep the raw synchronous CDP/unit-test contract unless explicitly requested.
  if (!transportEnvelope) return run();
  return Promise.resolve().then(run).then(
    value => ({blueDragonPageEnvelope:1,ok:true,value:value === undefined ? null : value}),
    error => ({blueDragonPageEnvelope:1,ok:false,error:String(error?.message || error).slice(0,300)})
  );
}
