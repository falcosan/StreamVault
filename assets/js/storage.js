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

const database = open(1);

export async function load(key) {
  const store = (await database).transaction(STORE).objectStore(STORE);
  return new Promise((resolve, reject) => {
    const request = store.get(key);
    request.onsuccess = () => resolve(request.result ?? null);
    request.onerror = () => reject(request.error);
  });
}

export async function save(key, value) {
  const transaction = (await database).transaction(STORE, "readwrite", {
    durability: "strict",
  });
  transaction.objectStore(STORE).put(value, key);
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = transaction.onabort = () => reject(transaction.error);
  });
}
