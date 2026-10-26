// Whole mouse counts from browser movement, which can be fractional (a
// high-resolution display or mouse reports parts of a pixel). Each event's
// fraction is carried into the next, so slow movement adds up instead of
// rounding away, as the native harness's RelativeMotion does.
export function relativeCounts() {
  let carried = [0, 0];
  return {
    take(dx, dy) {
      const x = dx + carried[0], y = dy + carried[1];
      const counts = [Math.round(x), Math.round(y)];
      carried = [x - counts[0], y - counts[1]];
      return counts;
    },
    reset() { carried = [0, 0]; },
  };
}

// Where a page position falls on the picture, in framebuffer pixels. `box`
// is the canvas's client rectangle and `size` its framebuffer; `contain` is
// true when the canvas letterboxes the picture (`object-fit: contain`).
export function toFramebuffer(box, size, contain, clientX, clientY) {
  let sx = box.width / size[0], sy = box.height / size[1], ox = 0, oy = 0;
  if (contain) {
    sx = sy = Math.min(sx, sy);
    ox = (box.width - size[0] * sx) / 2;
    oy = (box.height - size[1] * sy) / 2;
  }
  return [(clientX - box.left - ox) / sx, (clientY - box.top - oy) / sy];
}

// An Amiga mouse is relative input: the guest owns acceleration, clipping
// and warps, so its pointer cannot be put under the browser's cursor by
// subtracting a canvas offset. Two ways keep them together:
//
// - Following. While the machine can say where its pointer is (`setGuestPointer`),
//   hovering the screen sends the cursor's framebuffer position as a target
//   and hides the cursor, and the machine sends the counts that bring its
//   pointer there each frame. Clicks go to the guest.
// - Capture (pointer lock), the fallback: relative counts, and the only
//   choice while the machine cannot find its pointer, as in a game.
export function amigaMouse({canvas, button, hint, send, releaseKeys}) {
  let enabled = false, wasLocked = false, guestPointer = false, hovering = false;
  let queuedTarget = null;
  const motion = relativeCounts();
  const listeners = new AbortController();
  const listen = (target, type, handler, capture = false) => target.addEventListener(type, handler, {capture, signal:listeners.signal});
  const held = new Set();
  const locked = () => canvas.getRootNode().pointerLockElement === canvas;
  const following = () => enabled && guestPointer && !locked();
  const message = error => { hint.textContent = error; };
  function update() {
    button.disabled = !enabled;
    button.textContent = locked() ? 'Release mouse (Esc)' : 'Capture Amiga mouse';
    button.setAttribute('aria-pressed', String(locked()));
    hint.textContent = locked()
      ? 'Mouse captured. Use the Amiga pointer; press Escape to release.'
      : following()
        ? 'The Amiga pointer follows your cursor over the screen. Capture the mouse for games that move it themselves.'
        : 'Click the Amiga screen or Capture Amiga mouse. Escape releases it.';
    const hidden = following() && hovering;
    canvas.style.cursor = hidden ? 'none' : '';
    canvas.dataset.pointer = following() ? 'follows' : '';
  }
  function releaseButtons() {
    if (held.size) send([...held].map(name => ['mouse', name, false]));
    held.clear();
  }
  // Stop steering the guest pointer; it stays where it is.
  function leave() {
    queuedTarget = null;
    send([['leave']]);
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
  // One target a display frame is enough: the machine corrects once a frame.
  function target(event) {
    const box = canvas.getBoundingClientRect();
    const contain = getComputedStyle(canvas).objectFit === 'contain';
    const first = !queuedTarget;
    queuedTarget = toFramebuffer(box, [canvas.width, canvas.height], contain, event.clientX, event.clientY);
    if (first) requestAnimationFrame(() => {
      if (queuedTarget && following()) send([['target', ...queuedTarget]]);
      queuedTarget = null;
    });
  }
  listen(button, 'click', () => locked() ? release() : capture());
  listen(canvas, 'mousemove', event => {
    if (!following()) return;
    if (!hovering) { hovering = true; update(); }
    target(event);
  });
  listen(canvas, 'mouseleave', () => {
    if (!hovering) return;
    hovering = false; update();
    if (following()) leave();
  });
  listen(canvas, 'contextmenu', event => { if (following()) event.preventDefault(); });
  listen(canvas, 'mousedown', event => {
    if (!enabled || locked()) return;
    if (following()) {
      // The guest's pointer is under the cursor, so a click is the guest's.
      const name = ['left','middle','right'][event.button];
      if (!name || held.has(name)) return;
      event.preventDefault(); canvas.focus();
      held.add(name); send([['mouse', name, true]]);
      return;
    }
    if (event.button !== 0) return;
    event.preventDefault();
    capture(); // The activation click never clicks an unrelated guest control.
  });
  listen(document, 'pointerlockchange', () => {
    motion.reset(); // A new capture starts from a whole count.
    if (locked() && !enabled) { release(); return; }
    if (locked()) leave(); // Captured movement takes over from following.
    if (!locked() && wasLocked) { releaseButtons(); releaseKeys(); }
    wasLocked = locked();
    update();
  });
  listen(document, 'pointerlockerror', () => {
    message('Mouse capture failed. Click the screen to try again.');
  });
  listen(document, 'mousemove', event => {
    if (!enabled || !locked()) return;
    const [dx, dy] = motion.take(event.movementX, event.movementY);
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
      if (!value) { release(); hovering = false; }
      update();
    },
    // Whether the machine can say where its pointer is, from each frame.
    setGuestPointer(point) {
      const known = Boolean(point);
      if (known === guestPointer) return;
      const wasFollowing = following();
      guestPointer = known;
      // A pointer that has gone stops the machine's loop by itself; one that
      // appears under a cursor already over the screen starts following it.
      if (!wasFollowing && following()) hovering = false;
      if (wasFollowing && !following()) releaseButtons();
      update();
    },
    release,
    destroy() { release(); listeners.abort(); },
  };
}
