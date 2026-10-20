// ==UserScript==
// @name         StreamVault Sync
// @namespace    https://github.com/falcosan/StreamVault
// @version      1.0.0
// @description  Continue watching, progress and player preferences synced with StreamVault
// @include      /^https:\/\/(www\.)?streamingcommunity[^/]*\//
// @include      /^https:\/\/(www\.)?(vixcloud|vixsrc)[^/]*\//
// @grant        GM.xmlHttpRequest
// @grant        GM_xmlhttpRequest
// @connect      script.google.com
// @connect      script.googleusercontent.com
// @run-at       document-end
// ==/UserScript==

(() => {
  "use strict";

  const ENDPOINT = "https://script.google.com/macros/s/DEPLOYMENT_ID/exec";
  const TOKEN = "CHANGE_ME";
  const PROVIDER = 0;
  const PROVIDER_NAME = "StreamingCommunity";

  if (!/^(www\.)?(streamingcommunity|vixcloud|vixsrc)/i.test(location.hostname))
    return;

  const gmRequest =
    (typeof GM !== "undefined" && GM.xmlHttpRequest) ||
    (typeof GM_xmlhttpRequest !== "undefined" && GM_xmlhttpRequest) ||
    null;

  function gmFetch(method, url, body) {
    return new Promise((resolve, reject) => {
      gmRequest({
        method,
        url,
        data: body,
        anonymous: true,
        headers: body
          ? { "Content-Type": "text/plain;charset=utf-8" }
          : undefined,
        timeout: 30000,
        onload: (r) => resolve(r.responseText),
        onerror: () => reject(new Error("network error")),
        ontimeout: () => reject(new Error("timeout")),
      });
    });
  }

  function webFetch(method, url, body) {
    return fetch(url, {
      method,
      body,
      credentials: "omit",
      headers: body
        ? { "Content-Type": "text/plain;charset=utf-8" }
        : undefined,
    }).then((r) => r.text());
  }

  function parseJson(text) {
    try {
      return JSON.parse(text);
    } catch (_) {
      throw new Error(`non-JSON response: ${String(text).slice(0, 80)}`);
    }
  }

  async function http(method, url, body) {
    if (gmRequest) {
      try {
        return parseJson(await gmFetch(method, url, body));
      } catch (_) {}
    }
    try {
      return parseJson(await webFetch(method, url, body));
    } catch (err) {
      if (!gmRequest) throw err;
      await new Promise((r) => setTimeout(r, 1000));
      return parseJson(await gmFetch(method, url, body));
    }
  }

  const api = {
    async pull() {
      const res = await http(
        "GET",
        `${ENDPOINT}?token=${encodeURIComponent(TOKEN)}`,
      );
      if (!res.ok) throw new Error(res.error || "pull failed");
      return res.doc;
    },
    push(doc, watch, stamps) {
      return http(
        "POST",
        ENDPOINT,
        JSON.stringify({
          token: TOKEN,
          base_rev: doc.rev,
          watch,
          stamps,
          prefs: doc.prefs,
          prefs_updated_at: doc.prefs_updated_at,
        }),
      );
    },
  };

  const state = { doc: null };

  function setDoc(doc) {
    if (!doc || (state.doc && doc.rev < state.doc.rev && doc.rev > 1)) {
      return state.doc;
    }
    state.doc = doc;
    try {
      localStorage.setItem("sv:doc", JSON.stringify(doc));
    } catch (_) {}
    return doc;
  }

  function readCache() {
    try {
      const raw = localStorage.getItem("sv:doc");
      return raw ? JSON.parse(raw) : null;
    } catch (_) {
      return null;
    }
  }

  async function ensureDoc(force) {
    if (!state.doc || force) setDoc(await api.pull());
    return state.doc;
  }

  async function mutatePrefs(prefs, changed) {
    const keys = changed && changed.length ? changed : Object.keys(prefs);
    for (let attempt = 0; attempt < 3; attempt++) {
      const doc = await ensureDoc(attempt > 0);
      const merged = {
        audio_lang: "",
        subtitle_lang: "",
        subtitles_on: false,
        speed: 1,
        ...(doc.prefs || {}),
      };
      keys.forEach((k) => {
        if (k in prefs) merged[k] = prefs[k];
      });
      const res = await http(
        "POST",
        ENDPOINT,
        JSON.stringify({
          token: TOKEN,
          base_rev: doc.rev,
          watch: doc.watch || [],
          stamps: doc.stamps || {},
          prefs: merged,
          prefs_updated_at: Date.now(),
        }),
      );
      if (res.ok) return setDoc(res.doc);
      if (res.conflict) {
        setDoc(res.doc);
        continue;
      }
      throw new Error(res.error || "push failed");
    }
    return state.doc;
  }

  async function mutateWatch(mutator) {
    for (let attempt = 0; attempt < 3; attempt++) {
      const doc = await ensureDoc(attempt > 0);
      const watch = JSON.parse(JSON.stringify(doc.watch || []));
      const stamps = JSON.parse(JSON.stringify(doc.stamps || {}));
      if (!mutator(watch, stamps)) return doc;
      const res = await api.push(doc, watch, stamps);
      if (res.ok) {
        return setDoc(res.doc);
      }
      if (res.conflict) {
        setDoc(res.doc);
        continue;
      }
      throw new Error(res.error || "push failed");
    }
    return state.doc;
  }

  function runEmbed() {
    let ctx = null;
    let video = null;
    let lastSent = 0;
    let resumedKey = null;
    let prefsBase = null;
    let appliedAt = 0;

    function say(payload) {
      try {
        if (ctx && ctx.key) payload.key = ctx.key;
        window.top.postMessage(payload, "*");
      } catch (_) {}
    }

    const norm = (s) => (s || "").toString().trim().toLowerCase();
    const tag = (t) => norm((t.language || "") + " " + (t.label || ""));

    function pickIndex(tracks, wantLang) {
      if (wantLang) {
        for (let i = 0; i < tracks.length; i++) {
          if (tag(tracks[i]).includes(norm(wantLang))) return i;
        }
      }
      return 0;
    }

    function applyTracks(prefs) {
      const ats = video.audioTracks;
      if (prefs.audio_lang && ats && ats.length > 1) {
        const idx = pickIndex(ats, prefs.audio_lang);
        for (let i = 0; i < ats.length; i++) ats[i].enabled = i === idx;
      }
      const subs = subTracks();
      if (!subs.length) return;
      const idx = prefs.subtitles_on
        ? pickIndex(subs, prefs.subtitle_lang)
        : -1;
      for (let i = 0; i < subs.length; i++) {
        subs[i].mode = i === idx ? "showing" : "disabled";
      }
    }

    function subTracks() {
      const out = [];
      const tt = video.textTracks;
      if (tt) {
        for (let i = 0; i < tt.length; i++) {
          if (tt[i].kind === "subtitles" || tt[i].kind === "captions") {
            out.push(tt[i]);
          }
        }
      }
      return out;
    }

    function stateOf() {
      let showing = null;
      for (const t of subTracks()) if (t.mode === "showing") showing = t;
      let audio = "";
      const ats = video.audioTracks;
      if (ats) {
        for (let i = 0; i < ats.length; i++) {
          if (ats[i].enabled) {
            audio = ats[i].language || ats[i].label || "";
            break;
          }
        }
      }
      return {
        audio_lang: audio,
        subtitle_lang: showing ? showing.language || showing.label || "" : "",
        subtitles_on: !!showing,
        speed: video.playbackRate || 1,
      };
    }

    function reportPrefs() {
      if (!video || !ctx || !video.duration) return;
      if (prefsBase === null && ctx.prefs) return;
      const s = stateOf();
      const j = JSON.stringify(s);
      if (j === prefsBase) return;
      if (ctx.prefs && Date.now() - appliedAt < 5000) {
        apply();
        return;
      }
      const base = prefsBase ? JSON.parse(prefsBase) : null;
      const changed = base
        ? Object.keys(s).filter((k) => s[k] !== base[k])
        : Object.keys(s);
      prefsBase = j;
      if (!changed.length) return;
      say({ type: "sv:prefs", prefs: s, changed });
    }

    function apply() {
      if (!video || !ctx || !video.duration) return;
      const key = ctx.key || "";
      if (
        resumedKey !== key &&
        ctx.resume > 0 &&
        video.currentTime < 10 &&
        ctx.resume < video.duration - 5
      ) {
        video.currentTime = ctx.resume;
      }
      resumedKey = key;
      const prefs = ctx.prefs;
      if (prefs) {
        if (prefs.speed && prefs.speed !== 1) video.playbackRate = prefs.speed;
        applyTracks(prefs);
      }
      appliedAt = Date.now();
      prefsBase = JSON.stringify(stateOf());
    }

    function report(force) {
      if (!video || !video.duration || video.currentTime < 1) return;
      const now = Date.now();
      if (!force && now - lastSent < 5000) return;
      lastSent = now;
      say({
        type: "sv:time",
        current: video.currentTime,
        duration: video.duration,
      });
    }

    window.addEventListener("message", (ev) => {
      if (ev.source !== window.top) return;
      const msg = ev.data;
      if (!msg || msg.type !== "sv:ctx") return;
      ctx = msg;
      apply();
    });

    function attach(v) {
      if (video === v) return;
      video = v;
      video.addEventListener("loadedmetadata", apply);
      video.addEventListener("timeupdate", () => report(false));
      video.addEventListener("pause", () => report(true));
      video.addEventListener("ended", () => {
        report(true);
        say({ type: "sv:ended" });
      });
      const onPrefsEvent = () => setTimeout(reportPrefs, 100);
      video.addEventListener("ratechange", onPrefsEvent);
      if (video.textTracks) {
        video.textTracks.addEventListener("change", onPrefsEvent);
      }
      if (video.audioTracks) {
        video.audioTracks.addEventListener("change", onPrefsEvent);
      }
      say({ type: "sv:hello" });
      apply();
    }

    window.addEventListener("pagehide", () => report(true));

    const scan = () => {
      if (video && video.isConnected) return;
      const v = document.querySelector("video");
      if (v) attach(v);
    };
    scan();
    new MutationObserver(scan).observe(document.documentElement, {
      childList: true,
      subtree: true,
    });
    const helloTimer = setInterval(() => {
      if (ctx) {
        clearInterval(helloTimer);
        return;
      }
      if (video) say({ type: "sv:hello" });
    }, 1500);
  }

  let pageCache = null;

  function domPage() {
    try {
      const el = document.getElementById("app");
      return el && el.dataset.page ? JSON.parse(el.dataset.page) : null;
    } catch (_) {
      return null;
    }
  }

  function sameLocation(u) {
    if (!u) return false;
    try {
      const url = new URL(u, location.origin);
      return (
        url.pathname === location.pathname && url.search === location.search
      );
    } catch (_) {
      return false;
    }
  }

  function currentProps() {
    if (pageCache && sameLocation(pageCache.url)) return pageCache.props || {};
    const dom = domPage();
    if (dom && (!dom.url || sameLocation(dom.url))) return dom.props || {};
    return {};
  }

  async function fetchPage(urlPath) {
    const version =
      (pageCache && pageCache.version) || (domPage() || {}).version || "";
    const res = await fetch(urlPath, {
      headers: {
        "X-Inertia": "true",
        "X-Inertia-Version": version,
        Accept: "text/html, application/xhtml+xml",
      },
      credentials: "same-origin",
    });
    if (!res.ok) throw new Error("inertia fetch " + res.status);
    return res.json();
  }

  function watchContext() {
    const m = location.pathname.match(/^\/([a-z]{2})\/watch\/(\d+)/);
    if (!m) return null;
    const props = currentProps();
    const title = props.title || {};
    const episodeId = new URLSearchParams(location.search).get("e");
    const listed = (props.loadedSeason?.episodes || props.episodes || []).find(
      (e) => String(e.id) === episodeId,
    );
    const episode = props.episode || listed || null;
    const season =
      episode?.season?.number ??
      props.loadedSeason?.number ??
      props.season?.number ??
      null;
    const images = title.images || [];
    const poster =
      images.find((i) => i.type === "poster") ||
      images.find((i) => i.type === "cover") ||
      null;
    const rawEpisodeId = episode ? episode.id : episodeId;
    const epId = rawEpisodeId != null ? Number(rawEpisodeId) : NaN;
    const epDuration =
      episode?.duration != null ? Math.round(Number(episode.duration)) : NaN;
    return {
      lang: m[1],
      mediaId: Number(m[2]),
      hasTitle: typeof title.type === "string",
      name: title.name || document.title.replace(/\s*[-|].*$/, "").trim(),
      slug: title.slug || "",
      mediaType: title.type === "movie" ? "Movie" : "Series",
      imageUrl: poster
        ? `${location.protocol}//cdn.${location.host}/images/${poster.filename}`
        : null,
      year: title.release_date ? String(title.release_date).slice(0, 4) : null,
      score: title.score != null ? String(title.score) : null,
      season: season != null ? Number(season) : null,
      episode: Number.isFinite(epId)
        ? {
            id: Math.round(epId),
            number: Math.round(Number(episode?.number)) || 0,
            name: episode?.name || "",
            duration: Number.isFinite(epDuration) ? epDuration : null,
            image_url: null,
          }
        : null,
    };
  }

  function ctxKey(c) {
    return c ? `${c.mediaId}:${c.episode ? c.episode.id : ""}` : "";
  }

  function pickEpisode(current, previous) {
    if (!current) return previous;
    if (previous && previous.id === current.id && !current.number) {
      return previous;
    }
    return current;
  }

  function upsertProgress(ctx, current, duration) {
    return mutateWatch((watch, stamps) => {
      const key = `${PROVIDER}:${ctx.mediaId}`;
      const idx = watch.findIndex(
        (i) => i.entry.provider === PROVIDER && i.entry.id === ctx.mediaId,
      );
      const existing = idx >= 0 ? watch[idx] : null;
      if (!existing && !ctx.hasTitle) return false;
      const mediaType = existing ? existing.entry.media_type : ctx.mediaType;
      const finished = duration > 0 && duration - current <= 20;
      if (finished && mediaType === "Movie") {
        if (!existing) return false;
        watch.splice(idx, 1);
        delete stamps[key];
        return true;
      }
      const item = {
        entry: existing
          ? ctx.imageUrl && !existing.entry.image_url
            ? { ...existing.entry, image_url: ctx.imageUrl }
            : existing.entry
          : {
              id: ctx.mediaId,
              name: ctx.name,
              slug: ctx.slug,
              provider: PROVIDER,
              provider_name: PROVIDER_NAME,
              language: ctx.lang,
              media_type: ctx.mediaType,
              alternative_names: [],
              year: ctx.year,
              score: ctx.score,
              image_url: ctx.imageUrl,
              description: null,
            },
        current_time: current,
        duration,
        season: ctx.season ?? existing?.season ?? null,
        episode: pickEpisode(
          ctx.episode,
          existing && mediaType === "Series" ? existing.episode : null,
        ),
      };
      if (idx >= 0) watch.splice(idx, 1);
      watch.unshift(item);
      stamps[key] = Date.now();
      return true;
    });
  }

  const CSS = `
#sv-row.sv-fallback{margin:20px 0 4px;padding:0 16px;font-family:inherit}
#sv-row.sv-fallback .sv-head{color:#fff;font-size:1.55rem;font-weight:700;margin-bottom:12px;line-height:1.2}
#sv-row.sv-fallback .sv-scroll{display:flex;gap:10px;overflow-x:auto;-webkit-overflow-scrolling:touch;scrollbar-width:none;padding-bottom:4px}
#sv-row.sv-fallback .sv-scroll::-webkit-scrollbar{display:none}
#sv-row.sv-fallback .sv-card{position:relative;flex:0 0 clamp(105px,29vw,170px);aspect-ratio:2/3;border-radius:10px;overflow:hidden;background:#181818;text-decoration:none}
#sv-row.sv-fallback .sv-card img{width:100%;height:100%;object-fit:cover;display:block}
#sv-row .sv-ph{display:flex;align-items:center;justify-content:center;width:100%;height:100%;color:#aaa;font-size:12px;font-weight:600;text-align:center;padding:8px}
#sv-row .sv-ep{position:absolute;left:6px;bottom:10px;padding:2px 7px;border-radius:6px;background:rgba(0,0,0,.7);color:#fff;font-size:11px;font-weight:600;z-index:2;pointer-events:none}
#sv-row .sv-bar{position:absolute;left:0;right:0;bottom:0;height:4px;background:rgba(255,255,255,.25);z-index:2;pointer-events:none}
#sv-row .sv-bar div{height:100%;background:#fff;border-radius:0 2px 2px 0}
#sv-row .sv-x{position:absolute;top:5px;right:5px;width:30px;height:30px;border-radius:50%;border:none;background:rgba(0,0,0,.65);color:#fff;font-size:17px;line-height:30px;text-align:center;padding:0;cursor:pointer;z-index:3}
#sv-row .sv-off{opacity:.55}
`;

  function scopeAttrs(el) {
    return el
      ? [...el.attributes]
          .filter((a) => a.name.startsWith("data-v-"))
          .map((a) => a.name)
      : [];
  }

  function mk(tag, cls, scope) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    (scope || []).forEach((n) => e.setAttribute(n, ""));
    return e;
  }

  function removeItem(item, cardEl) {
    cardEl.remove();
    mutateWatch((watch, stamps) => {
      const idx = watch.findIndex(
        (i) =>
          i.entry.provider === item.entry.provider &&
          i.entry.id === item.entry.id,
      );
      if (idx < 0) return false;
      watch.splice(idx, 1);
      delete stamps[`${item.entry.provider}:${item.entry.id}`];
      return true;
    }).catch(() => {});
  }

  function attachOverlays(cardEl, item) {
    const x = document.createElement("button");
    x.className = "sv-x";
    x.title = "Remove";
    x.textContent = "×";
    x.svItem = item;
    x.svCard = cardEl;
    cardEl.appendChild(x);
    if (item.episode) {
      const ep = document.createElement("div");
      ep.className = "sv-ep";
      ep.textContent = `S${String(item.season ?? 1).padStart(2, "0")}E${String(item.episode.number).padStart(2, "0")}`;
      cardEl.appendChild(ep);
    }
    const pct =
      item.duration > 0
        ? Math.min(100, (item.current_time / item.duration) * 100)
        : 0;
    const bar = document.createElement("div");
    bar.className = "sv-bar";
    const fill = document.createElement("div");
    fill.style.width = `${pct}%`;
    bar.appendChild(fill);
    cardEl.appendChild(bar);
  }

  function itemHref(item) {
    const lang = item.entry.language || "it";
    const ep = item.episode ? `?e=${item.episode.id}` : "";
    return `/${lang}/watch/${item.entry.id}${ep}`;
  }

  function isScItem(item) {
    return (
      item.entry.provider === PROVIDER ||
      item.entry.provider_name === PROVIDER_NAME
    );
  }

  function nativeCard(item, tpl) {
    const isSc = isScItem(item);
    const cardEl = mk(
      "div",
      "ssr-title-card" + (isSc ? "" : " sv-off"),
      tpl.cardScope,
    );
    cardEl.style.position = "relative";
    const poster = fixImageUrl(item.entry.image_url);
    if (poster) {
      tpl.varNames.forEach((n) =>
        cardEl.style.setProperty(n, `url(${poster})`),
      );
    }
    const a = mk(isSc ? "a" : "div", "", tpl.aScope);
    if (isSc) a.href = itemHref(item);
    a.title = item.entry.name;
    const box = mk("div", "boxart", tpl.boxScope);
    if (!poster) {
      box.classList.add("sv-ph");
      box.textContent = item.entry.name;
    }
    a.appendChild(box);
    cardEl.appendChild(a);
    attachOverlays(cardEl, item);
    return cardEl;
  }

  function fixImageUrl(u) {
    if (!u) return u;
    try {
      const url = new URL(u);
      if (
        /^cdn\.streamingcommunity/i.test(url.hostname) &&
        /^(www\.)?streamingcommunity/i.test(location.hostname)
      ) {
        url.hostname = "cdn." + location.hostname.replace(/^www\./, "");
        return url.toString();
      }
    } catch (_) {}
    return u;
  }

  function fallbackCard(item) {
    const isSc = isScItem(item);
    const el = document.createElement(isSc ? "a" : "div");
    el.className = "sv-card" + (isSc ? "" : " sv-off");
    el.title = item.entry.name;
    if (isSc) el.href = itemHref(item);
    const poster = fixImageUrl(item.entry.image_url);
    const img = document.createElement(poster ? "img" : "div");
    if (poster) {
      img.loading = "lazy";
      img.src = poster;
      img.alt = item.entry.name;
    } else {
      img.className = "sv-ph";
      img.textContent = item.entry.name;
    }
    el.appendChild(img);
    attachOverlays(el, item);
    return el;
  }

  function findTemplate() {
    let template = null;
    let cardTpl = null;
    for (const row of document.querySelectorAll(".slider-row")) {
      if (row.id === "sv-row") continue;
      const c = row.querySelector(".ssr-title-card");
      if (c && c.querySelector(".boxart") && c.querySelector("a")) {
        template = row;
        cardTpl = c;
        break;
      }
    }
    if (!template) return null;
    return {
      rowScope: scopeAttrs(template),
      titleScope: scopeAttrs(template.querySelector(".row-title")),
      spanScope: scopeAttrs(template.querySelector(".row-title span")),
      sliderScope: scopeAttrs(template.querySelector(".slider")),
      peekScope: scopeAttrs(template.querySelector(".show-peek")),
      cardScope: scopeAttrs(cardTpl),
      aScope: scopeAttrs(cardTpl.querySelector("a")),
      boxScope: scopeAttrs(cardTpl.querySelector(".boxart")),
      varNames:
        (cardTpl.getAttribute("style") || "").match(/--[\w-]+(?=\s*:)/g) || [],
    };
  }

  let lastRendered = "";
  let lastMode = "";

  function renderRow(doc) {
    const items = (doc?.watch || [])
      .filter((i) => i && i.entry && typeof i.entry === "object")
      .slice(0, 20);
    const sig = JSON.stringify(
      items.map((i) => [
        i.entry.provider,
        i.entry.id,
        i.current_time,
        i.duration,
        i.episode?.id,
        i.season,
        i.entry.image_url,
      ]),
    );
    if (sig === lastRendered && document.getElementById("sv-row")) return;
    lastRendered = sig;
    document.getElementById("sv-row")?.remove();
    if (!items.length) return;
    const tpl = findTemplate();
    lastMode = tpl ? "native" : "fallback";
    const style = document.createElement("style");
    style.textContent = CSS;
    let wrap;
    if (tpl) {
      wrap = mk("div", "slider-row", tpl.rowScope);
      wrap.id = "sv-row";
      wrap.appendChild(style);
      const rt = mk(
        "div",
        "row-title",
        tpl.titleScope.length ? tpl.titleScope : tpl.rowScope,
      );
      const span = mk("span", "", tpl.spanScope);
      span.textContent = "Continue Watching";
      rt.appendChild(span);
      wrap.appendChild(rt);
      const slider = mk("div", "slider", tpl.sliderScope);
      const peek = mk("div", "show-peek", tpl.peekScope);
      for (const item of items) peek.appendChild(nativeCard(item, tpl));
      slider.appendChild(peek);
      wrap.appendChild(slider);
    } else {
      wrap = document.createElement("div");
      wrap.id = "sv-row";
      wrap.className = "sv-fallback";
      wrap.appendChild(style);
      const head = document.createElement("div");
      head.className = "sv-head";
      head.textContent = "Continue Watching";
      wrap.appendChild(head);
      const scroll = document.createElement("div");
      scroll.className = "sv-scroll";
      for (const item of items) scroll.appendChild(fallbackCard(item));
      wrap.appendChild(scroll);
    }
    const host = document.querySelector(".sliders");
    if (host && host.parentElement) {
      host.parentElement.insertBefore(wrap, host);
    } else {
      (document.querySelector("main") || document.body).prepend(wrap);
    }
  }

  function isHome() {
    return /^\/([a-z]{2})?\/?$/.test(location.pathname);
  }

  function runParent() {
    let ctx = null;
    let pendingTime = null;
    let lastPush = 0;
    let pushing = false;

    const stats = { hello: 0, time: 0, pushOk: 0, pushErr: null };
    let prefsTimer = 0;
    let prefsPending = null;
    let touchStart = null;

    const frames = new Set();

    function fromPlayerFrame(ev) {
      try {
        if (!ev.source || ev.source.top !== window) return false;
        for (const f of document.querySelectorAll("iframe")) {
          if (!/\/iframe|vixcloud|vixsrc/i.test(f.src || "")) continue;
          const w = f.contentWindow;
          if (w && (ev.source === w || ev.source.parent === w)) return true;
        }
        return false;
      } catch (_) {
        return false;
      }
    }

    async function flush(force) {
      if (!pendingTime || pushing) return;
      const now = Date.now();
      if (!force && now - lastPush < 10000) return;
      lastPush = now;
      pushing = true;
      const t = pendingTime;
      pendingTime = null;
      let failed = false;
      try {
        await upsertProgress(t.ctx, t.current, t.duration);
        stats.pushOk++;
      } catch (err) {
        failed = true;
        stats.pushErr = String((err && err.message) || err);
        pendingTime = pendingTime || t;
      }
      pushing = false;
      if (!failed && pendingTime) flush(true);
    }

    async function sendCtx(target) {
      if (!ctx) return;
      try {
        const doc = await ensureDoc(false);
        const item = (doc.watch || []).find(
          (i) => i.entry.provider === PROVIDER && i.entry.id === ctx.mediaId,
        );
        const sameEpisode =
          (!ctx.episode && !item?.episode) ||
          (ctx.episode && item?.episode && item.episode.id === ctx.episode.id);
        target.postMessage(
          {
            type: "sv:ctx",
            key: ctxKey(ctx),
            resume: item && sameEpisode ? item.current_time : 0,
            prefs: doc.prefs,
          },
          "*",
        );
      } catch (_) {}
    }

    window.addEventListener("message", (ev) => {
      const msg = ev.data;
      if (!msg || typeof msg.type !== "string" || !msg.type.startsWith("sv:"))
        return;
      if (!fromPlayerFrame(ev)) return;
      if (msg.type === "sv:hello") {
        stats.hello++;
        frames.add(ev.source);
        sendCtx(ev.source);
      } else if (msg.type === "sv:time") {
        stats.time++;
        if (
          !ctx ||
          msg.key !== ctxKey(ctx) ||
          !Number.isFinite(msg.current) ||
          !Number.isFinite(msg.duration) ||
          msg.current < 0 ||
          msg.duration <= 0
        )
          return;
        pendingTime = { ctx, current: msg.current, duration: msg.duration };
        flush(false);
      } else if (msg.type === "sv:ended") {
        if (ctx && msg.key === ctxKey(ctx)) flush(true);
      } else if (msg.type === "sv:prefs") {
        if (!ctx || msg.key !== ctxKey(ctx)) return;
        const p = msg.prefs;
        if (!p || typeof p !== "object") return;
        const clean = {
          audio_lang: String(p.audio_lang || ""),
          subtitle_lang: String(p.subtitle_lang || ""),
          subtitles_on: !!p.subtitles_on,
          speed: Number.isFinite(p.speed) && p.speed > 0 ? p.speed : 1,
        };
        const changed = (Array.isArray(msg.changed) ? msg.changed : []).filter(
          (k) => k in clean,
        );
        if (!prefsPending) {
          prefsPending = { prefs: clean, changed: new Set(changed) };
        } else {
          prefsPending.prefs = clean;
          changed.forEach((k) => prefsPending.changed.add(k));
        }
        clearTimeout(prefsTimer);
        prefsTimer = setTimeout(() => {
          const pending = prefsPending;
          prefsPending = null;
          if (!pending) return;
          mutatePrefs(pending.prefs, [...pending.changed]).catch(() => {});
        }, 2000);
      }
    });

    window.addEventListener("pagehide", () => flush(true));
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) flush(true);
    });

    let ctxFetchToken = 0;
    const posterCache = {};

    function ensurePoster() {
      if (!ctx || !ctx.hasTitle || !ctx.slug) return;
      const key = String(ctx.mediaId);
      if (posterCache[key] && posterCache[key] !== "pending") {
        if (!ctx.imageUrl) ctx.imageUrl = posterCache[key];
        return;
      }
      if (ctx.imageUrl || posterCache[key] === "pending") return;
      posterCache[key] = "pending";
      const mediaId = ctx.mediaId;
      fetchPage(`/${ctx.lang}/titles/${ctx.mediaId}-${ctx.slug}`)
        .then((p) => {
          const t = (p && p.props && p.props.title) || {};
          const imgs = t.images || [];
          const img =
            imgs.find((i) => i.type === "poster") ||
            imgs.find((i) => i.type === "cover") ||
            null;
          const cdn =
            (p && p.props && p.props.cdn_url) ||
            `${location.protocol}//cdn.${location.hostname.replace(/^www\./, "")}`;
          const url = img ? `${cdn}/images/${img.filename}` : null;
          posterCache[key] = url;
          if (url && ctx && ctx.mediaId === mediaId && !ctx.imageUrl) {
            ctx.imageUrl = url;
          }
        })
        .catch(() => {
          delete posterCache[key];
        });
    }

    let ctxFetchAt = 0;
    let ctxFetchedFor = "";

    function ensureCtxProps() {
      if (!/\/watch\/\d+/.test(location.pathname)) return;
      const episodeOk =
        ctx &&
        (ctx.mediaType === "Movie"
          ? !ctx.episode
          : !!(ctx.episode && ctx.episode.number));
      if (
        (ctx && ctx.hasTitle && episodeOk) ||
        ctxFetchedFor === location.href
      ) {
        ensurePoster();
        return;
      }
      const now = Date.now();
      if (now - ctxFetchAt < 3000) return;
      ctxFetchAt = now;
      const token = ++ctxFetchToken;
      fetchPage(location.pathname + location.search)
        .then((p) => {
          if (token !== ctxFetchToken || !p || !p.props) return;
          pageCache = p;
          ctxFetchedFor = location.href;
          ctx = watchContext();
          ensurePoster();
          frames.forEach((w) => sendCtx(w));
        })
        .catch(() => {});
    }

    function refresh() {
      frames.clear();
      ctx = watchContext();
      ensureCtxProps();
      if (!ctx && isHome()) {
        const cached = readCache();
        if (cached) renderRow(cached);
        ensureDoc(true)
          .then(renderRow)
          .catch(() => {});
      } else {
        document.getElementById("sv-row")?.remove();
        lastRendered = "";
      }
    }

    if (location.hash === "#svdebug") {
      ensureDoc(true)
        .then((doc) =>
          alert(
            `StreamVault Sync active\nEndpoint OK, rev ${doc.rev}, ${(doc.watch || []).length} titles`,
          ),
        )
        .catch((err) =>
          alert(
            `StreamVault Sync active\nEndpoint error: ${err && err.message}`,
          ),
        );
      setTimeout(() => {
        if (!ctx) return;
        alert(
          `Player debug\nctx ${ctx.mediaId}${ctx.episode ? " ep " + ctx.episode.id : ""}${ctx.hasTitle ? "" : " (missing props)"}\nhello: ${stats.hello} | time: ${stats.time}\npush ok: ${stats.pushOk} | err: ${stats.pushErr || "-"}`,
        );
      }, 10000);
    }

    let lastUrl = null;
    document.addEventListener("inertia:navigate", (ev) => {
      const p = ev && ev.detail && ev.detail.page;
      if (p && p.props) pageCache = p;
      lastUrl = location.href;
      flush(true);
      refresh();
    });
    const handleRemoveTap = (ev) => {
      const t = ev.target;
      const x = t && t.closest ? t.closest(".sv-x") : null;
      if (!x || !x.svItem) return;
      if (ev.type === "touchend") {
        const touch = ev.changedTouches && ev.changedTouches[0];
        if (
          touchStart &&
          touch &&
          Math.hypot(
            touch.clientX - touchStart.x,
            touch.clientY - touchStart.y,
          ) > 12
        ) {
          return;
        }
      }
      ev.preventDefault();
      ev.stopPropagation();
      const now = Date.now();
      if (x.svHandled && now - x.svHandled < 800) return;
      x.svHandled = now;
      removeItem(x.svItem, x.svCard || x.parentElement);
    };
    document.addEventListener(
      "touchstart",
      (ev) => {
        const touch = ev.touches && ev.touches[0];
        touchStart = touch ? { x: touch.clientX, y: touch.clientY } : null;
      },
      { capture: true, passive: true },
    );
    document.addEventListener("click", handleRemoveTap, true);
    document.addEventListener("touchend", handleRemoveTap, true);

    setInterval(() => {
      if (location.href !== lastUrl) {
        lastUrl = location.href;
        flush(true);
        setTimeout(refresh, 300);
        return;
      }
      if (ctx) {
        ensureCtxProps();
        return;
      }
      if (!isHome()) return;
      const doc = state.doc || readCache();
      if (!doc) return;
      if (!(doc.watch || []).length) {
        document.getElementById("sv-row")?.remove();
        return;
      }
      if (lastMode === "fallback" && findTemplate()) lastRendered = "";
      renderRow(doc);
    }, 800);

    lastUrl = location.href;
    refresh();
  }

  if (window.top !== window.self) {
    runEmbed();
  } else {
    runParent();
  }
})();
