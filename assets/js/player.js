const HAVE_FUTURE_DATA = 3;
const PROGRESSIVE = /\.(mp4|m4v|mov|webm|mkv)$/i;

const normalize = (text) =>
  String(text || "")
    .trim()
    .toLowerCase();
const describe = (track) =>
  normalize(`${track.language || ""} ${track.label || ""}`);
const pick = (tracks, lang) => {
  const wanted = normalize(lang);
  if (!wanted) return 0;
  const exact = tracks.findIndex((t) =>
    [t.language, t.label].some((value) => normalize(value) === wanted),
  );
  const index =
    exact >= 0 ? exact : tracks.findIndex((t) => describe(t).includes(wanted));
  return Math.max(index, 0);
};

const buffering = (video, started) =>
  video.paused
    ? !started
    : video.seeking || video.readyState < HAVE_FUTURE_DATA;

const track = (t) => ({
  language: t.lang || t.language || "",
  label: t.name || t.label || "",
});
const textTracks = (video) =>
  Array.from(video.textTracks || []).filter(
    (t) => t.kind === "subtitles" || t.kind === "captions",
  );

function nativePlayer(video) {
  return {
    audio: () => Array.from(video.audioTracks || []),
    audioIndex: () =>
      Array.from(video.audioTracks || []).findIndex((t) => t.enabled),
    subtitles: () => textTracks(video),
    subtitleIndex: () =>
      textTracks(video).findIndex((t) => t.mode === "showing"),
    selectAudio: (index) =>
      Array.from(video.audioTracks || []).forEach(
        (t, i) => (t.enabled = i === index),
      ),
    selectSubtitle: (index) =>
      textTracks(video).forEach(
        (t, i) => (t.mode = i === index ? "showing" : "disabled"),
      ),
  };
}

function hlsPlayer(hls) {
  let subtitle = -1;
  const showSubtitle = () => {
    hls.subtitleTrack = subtitle;
    hls.subtitleDisplay = subtitle >= 0;
  };
  hls.on(Hls.Events.AUDIO_TRACK_SWITCHED, showSubtitle);
  return {
    audio: () => hls.audioTracks,
    audioIndex: () => hls.audioTrack,
    subtitles: () => hls.subtitleTracks,
    subtitleIndex: () => (hls.subtitleDisplay ? hls.subtitleTrack : -1),
    selectAudio: (index) => (hls.audioTrack = index),
    selectSubtitle: (index) => {
      subtitle = index;
      showSubtitle();
    },
  };
}

function applyAudio(player, prefs) {
  const audio = player.audio().map(track);
  if (prefs.audio_lang && audio.length > 1)
    player.selectAudio(pick(audio, prefs.audio_lang));
}

function applySubtitles(player, prefs) {
  const subtitles = player.subtitles().map(track);
  if (subtitles.length)
    player.selectSubtitle(
      prefs.subtitles_on ? pick(subtitles, prefs.subtitle_lang) : -1,
    );
}

function attach(video, url, start, prefs, send) {
  let started = false;
  let player;
  const status = () =>
    send({
      type: "status",
      time: video.currentTime || 0,
      duration: Number.isFinite(video.duration) ? video.duration : 0,
      paused: video.paused,
      waiting: buffering(video, started),
    });
  const tracks = () =>
    send({
      type: "tracks",
      audio: player.audio().map(track),
      audio_index: player.audioIndex(),
      subtitles: player.subtitles().map(track),
      subtitle_index: player.subtitleIndex(),
    });
  const listeners = {
    timeupdate: status,
    durationchange: status,
    play: status,
    pause: status,
    waiting: status,
    playing: status,
    seeking: status,
    seeked: status,
    canplay: () => {
      started = true;
      status();
    },
    ended: () => send({ type: "ended" }),
    error: () =>
      send({ type: "failed", reason: `media error ${video.error?.code ?? 0}` }),
    loadedmetadata: () => {
      if (prefs.speed > 0) video.playbackRate = prefs.speed;
    },
  };
  for (const [name, listener] of Object.entries(listeners))
    video.addEventListener(name, listener);
  const unlisten = () => {
    for (const [name, listener] of Object.entries(listeners))
      video.removeEventListener(name, listener);
  };
  const session = (stop) => ({
    video,
    detach: () => {
      unlisten();
      stop();
    },
    select: (menu, index) => {
      if (menu === "audio") player.selectAudio(index);
      else player.selectSubtitle(index);
      tracks();
    },
  });
  status();

  if (PROGRESSIVE.test(url.split(/[?#]/)[0]) || !window.Hls?.isSupported()) {
    player = nativePlayer(video);
    const refresh = () => {
      applyAudio(player, prefs);
      applySubtitles(player, prefs);
      tracks();
    };
    const lists = [video.audioTracks, video.textTracks].filter(Boolean);
    for (const list of lists) list.addEventListener("addtrack", refresh);
    video.addEventListener(
      "loadedmetadata",
      () => {
        if (start > 0) video.currentTime = start;
        refresh();
      },
      { once: true },
    );
    video.src = url;
    video.play().catch(() => {});
    return session(() => {
      for (const list of lists) list.removeEventListener("addtrack", refresh);
      video.removeAttribute("src");
      video.load();
    });
  }

  const hls = new Hls({
    startPosition: start > 0 ? start : -1,
    subtitleDisplay: prefs.subtitles_on,
  });
  player = hlsPlayer(hls);
  let recovered = false;
  hls.on(Hls.Events.AUDIO_TRACKS_UPDATED, () => {
    applyAudio(player, prefs);
    tracks();
  });
  hls.on(Hls.Events.SUBTITLE_TRACKS_UPDATED, () => {
    applySubtitles(player, prefs);
    tracks();
  });
  for (const event of [
    Hls.Events.AUDIO_TRACK_SWITCHED,
    Hls.Events.SUBTITLE_TRACK_SWITCH,
  ])
    hls.on(event, tracks);
  hls.on(Hls.Events.ERROR, (_, data) => {
    if (!data.fatal) return;
    if (data.type === Hls.ErrorTypes.MEDIA_ERROR && !recovered) {
      recovered = true;
      hls.recoverMediaError();
      return;
    }
    hls.destroy();
    send({ type: "failed", reason: data.details });
  });
  hls.loadSource(url);
  hls.attachMedia(video);
  return session(() => hls.destroy());
}

function seek(video, time) {
  if (Number.isFinite(video.duration))
    video.currentTime = Math.min(Math.max(time, 0), video.duration);
}

function fullscreen(video) {
  if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  else
    video
      .closest(".player-screen")
      ?.requestFullscreen()
      .catch(() => {});
}

async function control(channel) {
  let session;
  const send = (event) => channel.send(event);
  for (;;) {
    const command = await channel.recv();
    if (command.type === "attach") {
      session?.detach();
      const video = document.querySelector(".player-video");
      session =
        video && attach(video, command.url, command.start, command.prefs, send);
      continue;
    }
    if (command.type === "detach") {
      session?.detach();
      session = undefined;
      continue;
    }
    const video = session?.video;
    if (!video) continue;
    if (command.type === "toggle")
      video.paused ? video.play().catch(() => {}) : video.pause();
    else if (command.type === "seek_to") seek(video, command.time);
    else if (command.type === "select")
      session.select(command.menu, command.index);
    else if (command.type === "fullscreen") fullscreen(video);
  }
}
