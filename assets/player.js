const [start, want] = await dioxus.recv();
const video = document.querySelector(".player-video");
if (!video || video.readyState < 2 || !Number.isFinite(video.duration)) {
  return null;
}

const normalize = (text) =>
  String(text || "")
    .trim()
    .toLowerCase();
const describe = (track) =>
  normalize(`${track.language || ""} ${track.label || ""}`);
const pick = (tracks, lang) =>
  Math.max(
    lang ? tracks.findIndex((t) => describe(t).includes(normalize(lang))) : 0,
    0,
  );

const audio = Array.from(video.audioTracks || []);
const subtitles = Array.from(video.textTracks || []).filter(
  (t) => t.kind === "subtitles" || t.kind === "captions",
);

const read = () => {
  const enabled = audio.find((t) => t.enabled);
  const showing = subtitles.find((t) => t.mode === "showing");
  return {
    audio_lang: enabled ? enabled.language || enabled.label || "" : "",
    subtitle_lang: showing ? showing.language || showing.label || "" : "",
    subtitles_on: Boolean(showing),
    speed: video.playbackRate || 1,
  };
};

if (!video.svPrefs) {
  if (start > 0) video.currentTime = start;
  video.play().catch(() => {});
  if (want.audio_lang && audio.length > 1) {
    const chosen = pick(audio, want.audio_lang);
    audio.forEach((t, i) => (t.enabled = i === chosen));
  }
  if (subtitles.length) {
    const chosen = want.subtitles_on ? pick(subtitles, want.subtitle_lang) : -1;
    subtitles.forEach(
      (t, i) => (t.mode = i === chosen ? "showing" : "disabled"),
    );
  }
  if (want.speed > 0) video.playbackRate = want.speed;
  video.svPrefs = read();
}

const current = read();
const changed = Object.fromEntries(
  Object.entries(current).filter(
    ([key, value]) => value !== video.svPrefs[key],
  ),
);
video.svPrefs = current;
return {
  time: video.currentTime,
  duration: video.duration,
  ended: video.ended,
  changed,
};
