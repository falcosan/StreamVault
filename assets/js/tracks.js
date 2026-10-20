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
