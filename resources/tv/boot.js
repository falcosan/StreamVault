(() => {
  const SHELL = 1;
  const RELEASES = "https://github.com/falcosan/StreamVault/releases";
  const PUBLIC_KEY =
    "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE5cKa7O1TH3Bf7BUECcNOmWH7dEIo5bh2WaHvIGtr7NRVd7tN+aFelM7+aWJrTsbyrC7plSl5uUDyY3ybN479Ig==";
  const DATABASE = "streamvault-update";
  const STORE = "update";
  const OPEN_ATTEMPTS = 20;
  const RETRY_MS = 250;
  const script = document.currentScript;
  const installed = {
    version: script.dataset.version,
    entry: script.dataset.entry,
  };

  const open = (attempt) =>
    new Promise((resolve, reject) => {
      const request = indexedDB.open(DATABASE, 1);
      request.onupgradeneeded = () => request.result.createObjectStore(STORE);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    }).catch(async (error) => {
      if (attempt >= OPEN_ATTEMPTS) throw error;
      await new Promise((resolve) => setTimeout(resolve, RETRY_MS));
      return open(attempt + 1);
    });

  const database = open(1);

  const read = async (key) => {
    const store = (await database).transaction(STORE).objectStore(STORE);
    return new Promise((resolve, reject) => {
      const request = store.get(key);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
  };

  const write = async (entries) => {
    const transaction = (await database).transaction(STORE, "readwrite", {
      durability: "strict",
    });
    const store = transaction.objectStore(STORE);
    for (const [key, value] of Object.entries(entries)) {
      if (value === undefined) store.delete(key);
      else store.put(value, key);
    }
    return new Promise((resolve, reject) => {
      transaction.oncomplete = () => resolve();
      transaction.onerror = transaction.onabort = () =>
        reject(transaction.error);
    });
  };

  const download = async (url, type) => {
    const response = await fetch(url, { cache: "no-store" });
    if (!response.ok) throw new Error(`${response.status} for ${url}`);
    return response[type]();
  };

  const hex = (buffer) =>
    [...new Uint8Array(buffer)]
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");

  const signed = async (manifest, signature) => {
    const key = await crypto.subtle.importKey(
      "spki",
      Uint8Array.from(atob(PUBLIC_KEY), (c) => c.charCodeAt(0)),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
    const valid = await crypto.subtle.verify(
      { name: "ECDSA", hash: "SHA-256" },
      key,
      signature,
      new TextEncoder().encode(manifest),
    );
    if (!valid) throw new Error("the update signature is not valid");
    return JSON.parse(manifest);
  };

  const matches = async (manifest, files) => {
    for (const { name, sha256 } of manifest.files) {
      const file = files[name];
      if (!file || hex(await crypto.subtle.digest("SHA-256", file)) !== sha256)
        throw new Error(`${name} does not match the signed update`);
    }
  };

  const verify = async (bundle) => {
    const manifest = await signed(bundle.manifest, bundle.signature);
    await matches(manifest, bundle.files);
    return manifest;
  };

  const blob = (data, type) => URL.createObjectURL(new Blob([data], { type }));

  const run = async (bundle, manifest) => {
    for (const name of manifest.scripts) {
      await new Promise((resolve, reject) => {
        const element = document.createElement("script");
        element.src = blob(bundle.files[name], "text/javascript");
        element.onload = resolve;
        element.onerror = reject;
        document.head.append(element);
      });
    }
    const wasm = `/./assets/${manifest.wasm}`;
    const entry = new TextDecoder().decode(bundle.files[manifest.entry]);
    if (!entry.includes(wasm))
      throw new Error(`${manifest.entry} does not load ${wasm}`);
    const module = entry.replace(
      wasm,
      blob(bundle.files[manifest.wasm], "application/wasm"),
    );
    await import(blob(module, "text/javascript"));
  };

  const startUpdate = async () => {
    const status = await read("status");
    if (status?.installed === installed.version && status.state !== "trying") {
      try {
        const bundle = await read("bundle");
        const manifest = await verify(bundle);
        if (status.state === "new")
          await write({ status: { ...status, state: "trying" } });
        await run(bundle, manifest);
        return true;
      } catch (error) {
        console.warn("StreamVault: the downloaded update cannot start", error);
      }
    }
    if (status) await write({ status: undefined, bundle: undefined });
    return false;
  };

  const start = async () => {
    try {
      if (await startUpdate()) return;
    } catch (error) {
      console.warn("StreamVault: the downloaded update cannot be read", error);
    }
    await import(installed.entry);
  };

  window.streamvault = {
    latest: async () =>
      JSON.parse(await download(`${RELEASES}/latest/download/tv.json`, "text"))
        .version,
    install: async () => {
      try {
        const text = await download(
          `${RELEASES}/latest/download/tv.json`,
          "text",
        );
        const base = `${RELEASES}/download/v${JSON.parse(text).version}`;
        const signature = await download(`${base}/tv.sig`, "arrayBuffer");
        const manifest = await signed(text, signature);
        if (manifest.shell > SHELL)
          throw new Error(
            `StreamVault ${manifest.version} needs a new installation: run scripts/tv/deploy.sh on your Mac`,
          );
        const files = {};
        for (const { name } of manifest.files)
          files[name] = await download(`${base}/${name}`, "arrayBuffer");
        await matches(manifest, files);
        await write({
          bundle: { manifest: text, signature, files },
          status: {
            version: manifest.version,
            installed: installed.version,
            state: "new",
          },
        });
        location.reload();
      } catch (error) {
        throw String(error?.message ?? error);
      }
    },
    confirm: async () => {
      const status = await read("status");
      if (status?.state === "trying")
        await write({ status: { ...status, state: "good" } });
    },
  };

  start();
})();
