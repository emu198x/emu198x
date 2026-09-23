// Preferences contain presentation choices only, never firmware or media.
const key = system => `emu198x-preferences-v1/${system}`;
export function readPreferences(system) {
  try { const value=JSON.parse(localStorage.getItem(key(system))); return value && typeof value==='object' ? value : {}; }
  catch { return {}; }
}
export function writePreferences(system, changes) {
  try { localStorage.setItem(key(system),JSON.stringify({...readPreferences(system),...changes})); return true; }
  catch { return false; }
}
