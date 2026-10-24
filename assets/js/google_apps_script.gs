const FILE_NAME = 'streamvault_state.json';
const MAX_CHANGES = 500;
const MAX_SKEW_MS = 60 * 1000;
const TOMBSTONE_TTL_MS = 90 * 24 * 60 * 60 * 1000;

function token_() {
  return PropertiesService.getScriptProperties().getProperty('TOKEN');
}

function out_(obj) {
  return ContentService.createTextOutput(JSON.stringify(obj)).setMimeType(
    ContentService.MimeType.JSON
  );
}

function emptyState_() {
  return { v: 2, seq: 0, floor: 0, entries: {} };
}

function file_() {
  const props = PropertiesService.getScriptProperties();
  const id = props.getProperty('FILE_ID');
  if (id) {
    try {
      const f = DriveApp.getFileById(id);
      if (!f.isTrashed()) return f;
    } catch (_) {}
  }
  const it = DriveApp.getFilesByName(FILE_NAME);
  while (it.hasNext()) {
    const f = it.next();
    if (!f.isTrashed()) {
      props.setProperty('FILE_ID', f.getId());
      return f;
    }
  }
  const f = DriveApp.createFile(
    FILE_NAME,
    JSON.stringify(emptyState_()),
    'application/json'
  );
  props.setProperty('FILE_ID', f.getId());
  return f;
}

function migrate_(legacy) {
  const state = emptyState_();
  const put = (key, value, at) => {
    state.entries[key] = { value, at, device: 'legacy', seq: ++state.seq };
  };
  const stamps = legacy.stamps || {};
  for (const item of Array.isArray(legacy.watch) ? legacy.watch : []) {
    if (!item || !item.entry) continue;
    const id = `${item.entry.provider}:${item.entry.id}`;
    put(
      `watch:${id}`,
      item,
      Number(stamps[id]) || Number(legacy.updated_at) || 0
    );
  }
  if (legacy.prefs)
    put('prefs', legacy.prefs, Number(legacy.prefs_updated_at) || 0);
  return state;
}

function readState_(f) {
  const text = f.getBlob().getDataAsString();
  let doc;
  try {
    doc = JSON.parse(text);
  } catch (_) {
    return { state: emptyState_(), migrated: true };
  }
  if (doc && doc.v === 2) return { state: doc, migrated: false };
  return { state: migrate_(doc || {}), migrated: true };
}

function wins_(a, b) {
  return a.at > b.at || (a.at === b.at && a.device > b.device);
}

function sync_(state, body, now) {
  const device = String(body.device || '');
  const changes = Array.isArray(body.changes)
    ? body.changes.slice(0, MAX_CHANGES)
    : [];
  const touched = new Set();
  let changed = false;
  for (const change of changes) {
    if (
      !change ||
      typeof change.key !== 'string' ||
      !Number.isFinite(change.at)
    )
      continue;
    touched.add(change.key);
    const next = {
      value: change.value === undefined ? null : change.value,
      at: Math.min(change.at, now + MAX_SKEW_MS),
      device,
    };
    const current = state.entries[change.key];
    if (current && !wins_(next, current)) continue;
    next.seq = ++state.seq;
    state.entries[change.key] = next;
    changed = true;
  }
  for (const [key, entry] of Object.entries(state.entries)) {
    if (
      entry.value === null &&
      now - entry.at > TOMBSTONE_TTL_MS &&
      !touched.has(key)
    ) {
      state.floor = Math.max(state.floor, entry.seq);
      delete state.entries[key];
      changed = true;
    }
  }
  const since = Number(body.since) || 0;
  const reset = since < state.floor || since > state.seq;
  const entries = Object.entries(state.entries)
    .filter(([key, entry]) => reset || entry.seq > since || touched.has(key))
    .map(([key, entry]) => ({
      key,
      value: entry.value,
      at: entry.at,
      device: entry.device,
    }));
  return { changed, response: { ok: true, seq: state.seq, reset, entries } };
}

function doPost(e) {
  let body;
  try {
    body = JSON.parse(e.postData.contents);
  } catch (_) {
    return out_({ ok: false, error: 'invalid body' });
  }
  if (!token_() || body.token !== token_()) {
    return out_({ ok: false, error: 'unauthorized' });
  }
  const lock = LockService.getScriptLock();
  if (!lock.tryLock(10000)) {
    return out_({ ok: false, error: 'busy' });
  }
  try {
    const f = file_();
    const { state, migrated } = readState_(f);
    const { changed, response } = sync_(state, body, Date.now());
    if (changed || migrated) f.setContent(JSON.stringify(state));
    return out_(response);
  } finally {
    lock.releaseLock();
  }
}
