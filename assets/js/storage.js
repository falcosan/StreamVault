const STORE = "state";
const OPEN_ATTEMPTS = 20;
const RETRY_MS = 250;

const open = (attempt) =>
  new Promise((resolve, reject) => {
    const request = indexedDB.open("streamvault", 1);
    request.onupgradeneeded = () => request.result.createObjectStore(STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  }).catch(async (error) => {
    if (attempt >= OPEN_ATTEMPTS) throw error;
    await new Promise((resolve) => setTimeout(resolve, RETRY_MS));
    return open(attempt + 1);
  });

const read = new Set();
let database;
const connect = () =>
  (database ??= open(1).catch((error) => {
    database = undefined;
    throw error;
  }));

export async function load(key) {
  const store = (await connect()).transaction(STORE).objectStore(STORE);
  return new Promise((resolve, reject) => {
    const request = store.get(key);
    request.onsuccess = () => {
      read.add(key);
      resolve(request.result ?? null);
    };
    request.onerror = () => reject(request.error);
  });
}

export async function packaged(name) {
  try {
    const response = await fetch(name);
    return response.ok ? await response.text() : null;
  } catch (_) {
    return null;
  }
}

export async function save(key, value) {
  if (!read.has(key)) throw new Error(`${key} was not read yet`);
  const transaction = (await connect()).transaction(STORE, "readwrite", {
    durability: "strict",
  });
  transaction.objectStore(STORE).put(value, key);
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = transaction.onabort = () => reject(transaction.error);
  });
}
