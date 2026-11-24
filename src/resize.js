// Adds a drag handle on the right edge of WhatsApp Web's chat list so it can be resized.
// Drag to resize, double-click the handle to reset. Width is remembered per account.
(function () {
  if (window.top !== window) return;
  var KEY = 'mwa-chatlist-width';
  var MIN = 220;
  var STYLE_KEYS = ['flex', 'width', 'maxWidth', 'minWidth'];
  var width = parseInt(localStorage.getItem(KEY) || '0', 10) || 0;
  var col = null, handle = null, dragging = false;
  // Other elements WhatsApp sizes like the chat list (e.g. the left overlay layer used for
  // "New chat"/"Settings"). If they keep the old width their border shows as a stray line.
  var companions = [];
  var lastScan = 0;

  // Walk up from #side to the element that is the chat-list column next to the conversation pane.
  function findColumn() {
    var n = document.getElementById('side') || document.getElementById('pane-side');
    while (n && n.parentElement && n.parentElement !== document.body) {
      var next = n.nextElementSibling;
      if (next) {
        var a = n.getBoundingClientRect(), b = next.getBoundingClientRect();
        if (a.width > 100 && b.width > 200 && Math.abs(a.right - b.left) < 4) return n;
      }
      n = n.parentElement;
    }
    return null;
  }

  function setWidth(el, w) {
    el.style.flex = '0 0 ' + w + 'px';
    el.style.width = w + 'px';
    el.style.maxWidth = 'none';
    el.style.minWidth = '0';
  }

  function clearWidth(el) {
    STYLE_KEYS.forEach(function (k) { el.style[k] = ''; });
  }

  // The chat list's rect as WhatsApp lays it out without our override.
  function naturalRect() {
    var saved = STYLE_KEYS.map(function (k) { return col.style[k]; });
    var grid = col.parentElement.style.gridTemplateColumns;
    clearWidth(col);
    col.parentElement.style.gridTemplateColumns = '';
    var r = col.getBoundingClientRect();
    STYLE_KEYS.forEach(function (k, i) { col.style[k] = saved[i]; });
    col.parentElement.style.gridTemplateColumns = grid;
    return r;
  }

  // Find tall elements outside the chat list that span the same horizontal range as it.
  function scanCompanions() {
    lastScan = Date.now();
    if (!col) return;
    var nat = naturalRect();
    var root = document.getElementById('app') || document.body;
    var all = root.getElementsByTagName('*');
    var found = [];
    for (var i = 0; i < all.length; i++) {
      var el = all[i];
      if (el === col || el === handle || col.contains(el) || el.contains(col)) continue;
      if (companions.indexOf(el) !== -1) { found.push(el); continue; }
      var r = el.getBoundingClientRect();
      if (r.height < window.innerHeight * 0.5) continue;
      if (Math.abs(r.left - nat.left) < 5 && Math.abs(r.right - nat.right) < 5) found.push(el);
    }
    // Keep only the outermost matches; their children follow them.
    found = found.filter(function (el) {
      return !found.some(function (o) { return o !== el && o.contains(el); });
    });
    companions.forEach(function (el) { if (found.indexOf(el) === -1) clearWidth(el); });
    companions = found;
  }

  function apply() {
    if (!col) return;
    var p = col.parentElement, ps = getComputedStyle(p);
    var w = Math.max(MIN, Math.min(width, window.innerWidth - 360));
    if (ps.display.indexOf('grid') !== -1) {
      var cols = ps.gridTemplateColumns.split(' ');
      var idx = Array.prototype.indexOf.call(p.children, col);
      if (idx >= 0 && idx < cols.length) {
        cols[idx] = w + 'px';
        for (var i = idx + 1; i < cols.length; i++) cols[i] = 'minmax(0, 1fr)';
        p.style.gridTemplateColumns = cols.join(' ');
      }
    }
    setWidth(col, w);
    companions.forEach(function (el) { setWidth(el, w); });
  }

  function reset() {
    width = 0;
    try { localStorage.removeItem(KEY); } catch (e) {}
    if (col) {
      clearWidth(col);
      col.parentElement.style.gridTemplateColumns = '';
    }
    companions.forEach(clearWidth);
    companions = [];
  }

  // Diagnostic: report layout (tags/classes/sizes only, no text) to the app, which saves it
  // to ~/Library/Application Support/MultiWhatsApp/layout-debug.json.
  function describe(el) {
    var r = el.getBoundingClientRect(), cs = getComputedStyle(el);
    return {
      tag: el.tagName.toLowerCase(), id: el.id || undefined,
      cls: (typeof el.className === 'string' ? el.className : '').slice(0, 120),
      x: Math.round(r.left), r: Math.round(r.right), y: Math.round(r.top), h: Math.round(r.height),
      pos: cs.position, disp: cs.display, br: cs.borderRightWidth + ' ' + cs.borderRightStyle,
      shadow: cs.boxShadow !== 'none' ? cs.boxShadow.slice(0, 60) : undefined,
      after: getComputedStyle(el, '::after').content !== 'none' ? 'yes' : undefined,
      before: getComputedStyle(el, '::before').content !== 'none' ? 'yes' : undefined
    };
  }

  function report() {
    if (!col || !window.ipc) return;
    var nat = naturalRect();
    var chain = [], n = col;
    while (n && n !== document.body) { chain.push(describe(n)); n = n.parentElement; }
    var siblings = Array.prototype.map.call(col.parentElement.children, describe);
    var atOld = [];
    for (var y = 0.2; y < 1; y += 0.3) {
      document.elementsFromPoint(nat.right - 1, window.innerHeight * y).slice(0, 12)
        .forEach(function (el) { atOld.push(describe(el)); });
    }
    try {
      window.ipc.postMessage(JSON.stringify({
        t: 'layout', inner: [window.innerWidth, window.innerHeight], width: width,
        natural: [Math.round(nat.left), Math.round(nat.right)],
        chain: chain, siblings: siblings, companions: companions.map(describe), atOldEdge: atOld
      }));
    } catch (e) {}
  }

  function makeHandle() {
    handle = document.createElement('div');
    handle.title = 'Drag to resize chat list — double-click to reset';
    handle.style.cssText = 'position:fixed;top:0;width:8px;margin-left:-4px;cursor:col-resize;' +
      'z-index:2147483647;background:transparent;transition:background .15s;display:none';
    handle.onmouseenter = function () { handle.style.background = 'rgba(37,211,102,.45)'; };
    handle.onmouseleave = function () { if (!dragging) handle.style.background = 'transparent'; };
    handle.ondblclick = function () { reset(); position(); };
    handle.addEventListener('pointerdown', function (e) {
      if (!col) return;
      scanCompanions();
      dragging = true;
      handle.setPointerCapture(e.pointerId);
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });
    handle.addEventListener('pointermove', function (e) {
      if (!dragging || !col) return;
      width = Math.max(MIN, Math.min(Math.round(e.clientX - col.getBoundingClientRect().left),
        window.innerWidth - 360));
      apply();
      position();
    });
    function end() {
      if (!dragging) return;
      dragging = false;
      document.body.style.userSelect = '';
      handle.style.background = 'transparent';
      try { localStorage.setItem(KEY, String(width)); } catch (err) {}
      report();
    }
    handle.addEventListener('pointerup', end);
    handle.addEventListener('pointercancel', end);
    document.body.appendChild(handle);
  }

  function position() {
    if (!handle) return;
    if (!col) { handle.style.display = 'none'; return; }
    var r = col.getBoundingClientRect();
    handle.style.display = 'block';
    handle.style.left = r.right + 'px';
    handle.style.top = r.top + 'px';
    handle.style.height = r.height + 'px';
  }

  function tick() {
    if (!document.body) return;
    if (!handle || !handle.isConnected) makeHandle();
    if (!col || !col.isConnected || !col.nextElementSibling) {
      col = findColumn();
      companions = [];
      if (col && width) { scanCompanions(); apply(); }
    } else if (width && !dragging && Date.now() - lastScan > 2500) {
      // WhatsApp mounts overlay layers lazily; pick up new ones.
      scanCompanions();
      apply();
    }
    if (!dragging) position();
  }

  setInterval(tick, 700);
  window.addEventListener('resize', function () {
    if (col && width) { scanCompanions(); apply(); }
    position();
  });
})();
