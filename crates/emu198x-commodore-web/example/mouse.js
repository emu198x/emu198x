// An Amiga mouse is relative input. Its guest cursor cannot be aligned to a
// browser's absolute cursor by subtracting a canvas offset (the guest owns
// acceleration, clipping and warps). Capture exposes only the guest cursor.
export function amigaMouse({canvas, button, hint, send, releaseKeys}) {
  let enabled = false, wasLocked = false;
  const listeners = new AbortController();
  const listen = (target, type, handler, capture = false) => target.addEventListener(type, handler, {capture, signal:listeners.signal});
  const held = new Set();
  const locked = () => canvas.getRootNode().pointerLockElement === canvas;
  const message = error => { hint.textContent = error; };
  function update() {
    button.disabled = !enabled;
    button.textContent = locked() ? 'Release mouse (Esc)' : 'Capture Amiga mouse';
    button.setAttribute('aria-pressed', String(locked()));
    hint.textContent = locked()
      ? 'Mouse captured. Use the Amiga pointer; press Escape to release.'
      : 'Click the Amiga screen or Capture Amiga mouse. Escape releases it.';
  }
  function releaseButtons() {
    if (held.size) send([...held].map(name => ['mouse', name, false]));
    held.clear();
  }
  function release() {
    releaseButtons();
    if (locked()) document.exitPointerLock();
  }
  function capture() {
    if (!enabled || locked()) return;
    if (!canvas.requestPointerLock) {
      message('Mouse capture is unavailable in this browser. Try a desktop browser with Pointer Lock support.');
      return;
    }
    canvas.focus();
    try {
      // Both the promise and older event-only browser APIs are supported.
      canvas.requestPointerLock()?.catch(() => message('Mouse capture failed. Click the screen to try again.'));
    } catch {
      message('Mouse capture failed. Click the screen to try again.');
    }
  }
  listen(button, 'click', () => locked() ? release() : capture());
  listen(canvas, 'mousedown', event => {
    if (!enabled || locked() || event.button !== 0) return;
    event.preventDefault();
    capture(); // The activation click never clicks an unrelated guest control.
  });
  listen(document, 'pointerlockchange', () => {
    if (locked() && !enabled) { release(); return; }
    if (!locked() && wasLocked) { releaseButtons(); releaseKeys(); }
    wasLocked = locked();
    update();
  });
  listen(document, 'pointerlockerror', () => {
    message('Mouse capture failed. Click the screen to try again.');
  });
  listen(document, 'mousemove', event => {
    if (!enabled || !locked()) return;
    const dx = Math.round(event.movementX), dy = Math.round(event.movementY);
    if (dx || dy) send([['move', dx, dy]]);
  });
  // Capture phase runs before the activation handler above: an unlocked click
  // cannot turn into a guest button press even if capture completes promptly.
  listen(document, 'mousedown', event => {
    if (!enabled || !locked()) return;
    const name = ['left','middle','right'][event.button];
    if (!name || held.has(name)) return;
    event.preventDefault(); held.add(name); send([['mouse',name,true]]);
  }, true);
  listen(document, 'mouseup', event => {
    const name = ['left','middle','right'][event.button];
    if (!held.delete(name)) return;
    event.preventDefault(); send([['mouse',name,false]]);
  });
  listen(document, 'keydown', event => {
    if (event.code !== 'Escape' || !locked()) return;
    event.stopImmediatePropagation(); // Escape releases the host, not the guest.
    release();
  }, true);
  listen(window, 'blur', release);
  update();
  return {
    setEnabled(value) {
      enabled = value;
      if (!value) release();
      update();
    },
    release,
    destroy() { release(); listeners.abort(); },
  };
}
