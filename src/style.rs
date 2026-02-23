pub const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 1200"><path fill="#f4fd37" d="M458.8 436.7C386 512 320.6 579.5 313.6 586.7L300.8 600l144.8-.1h144.9l137.2-134.2c75.4-73.8 144.4-141.3 153.2-150L897 300H591.4zm0 300C386 812 320.6 879.5 313.6 886.7L300.8 900l144.8-.1h144.9l137.2-134.2c75.4-73.8 144.4-141.3 153.2-150L897 600H591.4z"/></svg>"##;

pub const GLOBAL_CSS: &str = r#"
:root {
    --bg: #151515; --surface: #1c1c1c; --surface2: #272727;
    --border: #333333; --accent: #f4fd37; --accent-hover: #d4dd17;
    --accent-text: #151515;
    --warn: #f5b014; --danger: #e53935; --success: #46d369;
    --text: #e5e5e5; --text2: #a0a0a0; --text3: #686868;
    --navbar: #0d0d0d;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: var(--bg); color: var(--text); font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }
::-webkit-scrollbar { display: none; }
body { scrollbar-width: none; -ms-overflow-style: none; }
.app { display: flex; flex-direction: column; height: 100vh; }

.navbar {
    display: flex; align-items: center; gap: 2px; padding: 0 20px;
    height: 56px; min-height: 56px; background: var(--navbar);
    -webkit-app-region: drag;
}
.navbar button, .navbar input, .navbar .search-bar { -webkit-app-region: no-drag; }
.logo { background: none; border: none; cursor: pointer; padding: 0; display: flex; align-items: center; }
.logo:hover { opacity: 0.8; }
.logo-icon { height: 30px; width: 30px; }
.logo-icon svg { width: 100%; height: 100%; display: block; }
.nav-spacer { width: 10px; }
.nav-link {
    background: none; border: none; color: #808080; font-size: 13px;
    padding: 4px 10px; cursor: pointer; border-radius: 3px;
}
.nav-link:hover { background: #303030; color: #b0b0b0; }
.nav-link.active { color: var(--accent); }
.nav-fill { flex: 1; }
.search-bar {
    display: flex; align-items: center; background: var(--surface2);
    border-radius: 22px; padding: 2px 2px 2px 14px; gap: 6px;
    border: 1px solid var(--border); transition: border-color 0.2s;
}
.search-bar:focus-within { border-color: var(--accent); }
.search-input {
    background: transparent; border: none; color: var(--text);
    padding: 6px 0; font-size: 13px; width: 200px; outline: none;
}
.search-go {
    background: var(--surface); border: none; color: var(--text2);
    width: 32px; height: 32px; border-radius: 50%; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    transition: background 0.15s, color 0.15s; flex-shrink: 0;
}
.search-go:hover { background: var(--accent); color: var(--accent-text); }
.search-go:disabled { opacity: 0.5; cursor: default; }
.search-icon { width: 15px; height: 15px; display: flex; }
.search-icon svg { width: 100%; height: 100%; display: block; }

.content { flex: 1; overflow-y: auto; }

.error-bar {
    display: flex; align-items: center; gap: 8px; padding: 8px 20px;
    background: var(--danger); color: white; font-size: 13px;
}
.error-bar .dismiss { background: none; border: none; color: white; cursor: pointer; font-size: 12px; padding: 4px 8px; }
.error-bar .fill { flex: 1; }

.center-msg {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    height: 100%; text-align: center; padding: 20px;
}

.splash-screen {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    height: 100%; gap: 16px;
}
.splash-logo { width: 100px; height: 100px; }
.splash-logo svg { width: 100%; height: 100%; display: block; }

.catalog-view { padding: 16px 0 20px; }
.section-header { display: flex; align-items: center; gap: 10px; padding: 0 20px 12px; }
.section-title { font-size: 18px; color: var(--text); }
.section-count { font-size: 12px; color: var(--text3); }

.media-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 16px;
    padding: 0 20px 20px;
}

.poster-card {
    width: 100%; aspect-ratio: 2/3; border-radius: 6px;
    cursor: pointer; position: relative; overflow: hidden;
    border: none; padding: 0; text-align: left;
    background-size: cover; background-position: center;
    transition: transform 0.15s;
}
.poster-card:hover { transform: scale(1.03); }
.poster-overlay {
    position: absolute; bottom: 0; left: 0; right: 0;
    background: linear-gradient(transparent, rgba(0,0,0,0.85)); padding: 8px 10px;
}
.poster-title { font-size: 12px; color: white; margin-bottom: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.poster-meta { display: flex; align-items: center; gap: 6px; }
.badge {
    font-size: 8px; padding: 1px 6px; border-radius: 2px;
    text-transform: uppercase; font-weight: bold;
}
.badge-movie { background: var(--accent); color: var(--accent-text); }
.badge-series { background: #0091d5; color: white; }
.poster-year { font-size: 10px; color: #b0b0b0; }
.poster-score {
    position: absolute; top: 6px; right: 6px;
    font-size: 10px; font-weight: 700; color: var(--accent-text);
    background: var(--accent); padding: 2px 6px; border-radius: 3px;
    z-index: 1;
}

.empty-msg { font-size: 16px; color: var(--text3); }
.searching-msg { font-size: 16px; color: var(--text3); }

.details-toolbar { display: flex; align-items: center; gap: 8px; padding: 10px 24px; }
.btn-ghost {
    background: transparent; border: 1px solid var(--border); color: var(--text);
    padding: 6px 14px; font-size: 13px; cursor: pointer; border-radius: 3px;
}
.btn-ghost:hover { background: var(--surface2); }
.btn-accent {
    background: var(--accent); border: none; color: var(--accent-text);
    padding: 10px 20px; font-size: 14px; font-weight: 600; cursor: pointer; border-radius: 3px; min-width: 140px; text-align: center;
}
.btn-accent:hover { background: var(--accent-hover); }

.details-header {
    display: flex; gap: 32px; padding: 8px 32px 24px;
}
.details-info {
    flex: 1; display: flex; flex-direction: column; gap: 8px; min-width: 0;
}
.details-title {
    font-size: 34px; font-weight: bold; color: white; line-height: 1.15;
}
.details-meta {
    display: flex; align-items: center; gap: 10px;
}
.details-kind-badge { font-size: 10px; padding: 2px 10px; border-radius: 3px; }
.details-year { font-size: 13px; color: #bbbbbb; }
.details-actions { display: flex; align-items: center; gap: 10px; margin-top: 6px; }
.details-desc {
    font-size: 14px; color: var(--text2); line-height: 1.6; margin-top: 4px; max-width: 600px;
}
.details-score {
    font-size: 13px; font-weight: 700; color: var(--accent); margin-top: 4px;
}
.details-poster {
    width: 200px; flex-shrink: 0;
}
.details-poster img {
    width: 100%; border-radius: 8px; display: block;
}
.details-poster-placeholder {
    width: 100%; aspect-ratio: 2/3; border-radius: 8px;
    display: flex; align-items: center; justify-content: center;
    font-size: 48px; color: rgba(255,255,255,0.3);
}

.season-tabs { display: flex; gap: 6px; padding: 6px 24px; flex-wrap: wrap; }
.season-tab {
    font-size: 12px; padding: 7px 14px; cursor: pointer; border-radius: 3px;
    border: 1px solid var(--border); background: transparent; color: var(--text2);
}
.season-tab:hover { background: var(--surface2); }
.season-tab.active { background: var(--accent); border-color: var(--accent); color: var(--accent-text); font-weight: 600; cursor: default; }

.episodes-list { padding: 4px 24px; display: flex; flex-direction: column; gap: 3px; }
.episode-row {
    display: flex; align-items: center; gap: 12px; padding: 8px 12px;
    background: var(--surface); border-radius: 4px; cursor: pointer; border: none;
    width: 100%; text-align: left; color: var(--text);
}
.episode-row:hover { background: var(--surface2); }
.ep-num { font-size: 18px; color: var(--text3); width: 30px; text-align: center; flex-shrink: 0; }
.ep-info { flex: 1; }
.ep-name { font-size: 13px; color: var(--text); }
.ep-dur { font-size: 11px; color: var(--text3); margin-top: 2px; }
.ep-play {
    background: var(--accent); border: none; color: var(--accent-text);
    padding: 7px 14px; font-size: 12px; font-weight: 600; cursor: pointer; border-radius: 3px;
}
.ep-play:hover { background: var(--accent-hover); }
.ep-dl {
    background: transparent; border: 1px solid var(--border); color: var(--text2);
    padding: 7px 10px; font-size: 12px; cursor: pointer; border-radius: 3px;
}
.ep-dl:hover { background: var(--surface2); }

.player-screen { display: flex; flex-direction: column; height: 100%; background: #000; }
.player-top-bar {
    display: flex; align-items: center; gap: 12px; padding: 10px 24px;
    background: rgba(20,20,20,0.95); z-index: 1;
}
.player-title-text { font-size: 14px; color: var(--text); flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.player-video-container { flex: 1; display: flex; align-items: center; justify-content: center; background: #000; min-height: 0; }
.player-video { width: 100%; height: 100%; object-fit: contain; outline: none; }
.btn-next-episode {
    background: var(--accent); border: none; color: var(--accent-text);
    padding: 6px 14px; font-size: 13px; font-weight: 600; cursor: pointer; border-radius: 3px;
    white-space: nowrap;
}
.btn-next-episode:hover { background: var(--accent-hover); }

.dl-header { display: flex; align-items: center; padding: 14px 20px; }
.dl-title { font-size: 18px; color: var(--text); flex: 1; }
.dl-count { font-size: 12px; color: var(--text3); }
.dl-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; }
.dl-empty-title { font-size: 16px; color: var(--text2); }
.dl-empty-sub { font-size: 13px; color: var(--text3); margin-top: 6px; }
.dl-list { padding: 4px 20px; display: flex; flex-direction: column; gap: 6px; }
.dl-card { background: var(--surface); border-radius: 4px; padding: 14px; }
.dl-card-top { display: flex; align-items: center; }
.dl-card-title { font-size: 13px; color: var(--text); flex: 1; }
.dl-card-status { font-size: 11px; }

.loading-msg { font-size: 13px; color: var(--text3); padding: 10px 24px; }
"#;
