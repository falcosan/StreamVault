const FILE_NAME = 'streamvault_state.json';

function token_() {
  return PropertiesService.getScriptProperties().getProperty('TOKEN');
}

function out_(obj) {
  return ContentService.createTextOutput(JSON.stringify(obj)).setMimeType(
    ContentService.MimeType.JSON
  );
}

function emptyDoc_() {
  return {
    rev: 0,
    updated_at: 0,
    prefs_updated_at: 0,
    watch: [],
    stamps: {},
    prefs: null,
  };
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
    JSON.stringify(emptyDoc_()),
    'application/json'
  );
  props.setProperty('FILE_ID', f.getId());
  return f;
}

function readDoc_(f) {
  try {
    const doc = JSON.parse(f.getBlob().getDataAsString());
    return doc && typeof doc.rev === 'number' ? doc : emptyDoc_();
  } catch (_) {
    return emptyDoc_();
  }
}

function writeDoc_(f, doc) {
  f.setContent(JSON.stringify(doc));
}

function doGet(e) {
  if (!e || !e.parameter || !token_() || e.parameter.token !== token_()) {
    return out_({ ok: false, error: 'unauthorized' });
  }
  return out_({ ok: true, doc: readDoc_(file_()) });
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
    const doc = readDoc_(f);
    if (Number(body.base_rev) !== doc.rev) {
      return out_({ ok: false, conflict: true, doc: doc });
    }
    const next = {
      rev: doc.rev + 1,
      updated_at: Date.now(),
      prefs_updated_at:
        typeof body.prefs_updated_at === 'number'
          ? body.prefs_updated_at
          : doc.prefs_updated_at,
      watch: Array.isArray(body.watch) ? body.watch : doc.watch,
      stamps:
        body.stamps && typeof body.stamps === 'object'
          ? body.stamps
          : doc.stamps || {},
      prefs: body.prefs || doc.prefs,
    };
    writeDoc_(f, next);
    return out_({ ok: true, doc: next });
  } finally {
    lock.releaseLock();
  }
}
