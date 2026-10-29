#[cfg(feature = "desktop")]
use super::Desktop;
use super::Text;

pub static TEXT: Text = Text {
    english: "Inglese",
    italian: "Italiano",
    language: "Lingua",
    forced: "Forzati",
    decimal: ",",
    back: "Indietro",
    home: "Home",
    loading: "Caricamento",
    search: "Cerca",
    search_placeholder: "Cerca...",
    submit_search: "Avvia la ricerca",
    updating: "Aggiornamento in corso",
    install_update: "Installa l'aggiornamento",
    update_failed: "Aggiornamento non riuscito",
    dismiss_error: "Chiudi l'errore",
    continue_watching: "Continua a guardare",
    remove_from_continue_watching: |name| format!("Rimuovi {name} da Continua a guardare"),
    no_media: "Nessun contenuto disponibile",
    retry: "Riprova",
    loading_more: "Caricamento di altri titoli...",
    searching: "Ricerca in corso...",
    no_results: "Nessun risultato",
    results: "Risultati",
    result_count: |n| match n {
        1 => "1 risultato".into(),
        n => format!("{n} risultati"),
    },
    movie: "Film",
    series: "Serie",
    season_count: |n| match n {
        1 => "1 stagione".into(),
        n => format!("{n} stagioni"),
    },
    seasons: "Stagioni",
    season: |n| format!("Stagione {n}"),
    episodes: "Episodi",
    episode: |n| format!("Episodio {n}"),
    no_episodes: "Nessun episodio disponibile",
    play: "Riproduci",
    resume: "Riprendi",
    play_episode: |n, title| format!("Riproduci l'episodio {n}: {title}"),
    time_left: |time| format!("Mancano {time}"),
    episodes_failed: "Impossibile caricare gli episodi",
    next_season_failed: "Impossibile caricare la stagione successiva",
    stream_failed: "Impossibile ottenere il video",
    playback_failed: "Riproduzione non riuscita",
    pause: "Pausa",
    seek: "Posizione",
    time_of: |time, total| format!("{time} di {total}"),
    audio: "Audio",
    subtitles: "Sottotitoli",
    audio_track: |name| format!("Audio: {name}"),
    subtitles_track: |name| format!("Sottotitoli: {name}"),
    default_track: "Predefinito",
    off: "Disattivati",
    next_episode: "Episodio successivo",
    fullscreen: "Schermo intero",
    exit_fullscreen: "Esci dallo schermo intero",
    #[cfg(feature = "desktop")]
    desktop: Desktop {
        downloads: "Download",
        download: "Scarica",
        queued: "In coda",
        progress: |percent| format!("Download al {percent}%"),
        finishing: "Finalizzazione",
        downloaded: "Scaricato",
        retry: "Riprova il download",
        download_this: |episode| match episode {
            Some(n) => format!("Scarica l'episodio {n}"),
            None => "Scarica questo film".into(),
        },
        play_this: |episode| match episode {
            Some(n) => format!("Riproduci l'episodio {n}"),
            None => "Riproduci questo film".into(),
        },
        retry_this: |episode| match episode {
            Some(n) => format!("Riprova il download dell'episodio {n}"),
            None => "Riprova il download di questo film".into(),
        },
        show_this: |episode| match episode {
            Some(n) => format!("Mostra il download dell'episodio {n}"),
            None => "Mostra il download di questo film".into(),
        },
        empty: "Ancora nessun download",
        empty_hint: "Scarica un film o un episodio dalla sua pagina e guardalo qui, anche offline.",
        browse: "Esplora i titoli",
        active: "In download",
        ready_to_watch: "Pronti da guardare",
        waiting: "In attesa di iniziare",
        progress_of: |title| format!("Avanzamento del download di {title}"),
        finishing_of: |title| format!("Finalizzazione di {title}"),
        failed: |error| format!("Download non riuscito: {error}"),
        reveal: if cfg!(windows) {
            "Mostra in Esplora file"
        } else {
            "Mostra nel Finder"
        },
        reveal_title: |title| {
            if cfg!(windows) {
                format!("Mostra {title} in Esplora file")
            } else {
                format!("Mostra {title} nel Finder")
            }
        },
        reveal_failed: if cfg!(windows) {
            "Impossibile mostrare in Esplora file"
        } else {
            "Impossibile mostrare nel Finder"
        },
        play_title: |title| format!("Riproduci {title}"),
        delete: "Elimina",
        delete_title: |title| format!("Elimina {title}"),
        retry_title: |title| format!("Riprova {title}"),
        remove: "Rimuovi",
        remove_title: |title| format!("Rimuovi {title}"),
        cancel: "Annulla",
        cancel_title: |title| format!("Annulla {title}"),
        ready: "Pronto",
        running: |n| format!("{n} in download"),
        failed_count: |n| match n {
            1 => "1 non riuscito".into(),
            n => format!("{n} non riusciti"),
        },
        titles_ready: |n| match n {
            1 => "1 titolo pronto".into(),
            n => format!("{n} titoli pronti"),
        },
        delete_failed: "Impossibile eliminare il download",
        file_missing: "File non trovato",
        restart: "Aggiornamento installato, riavvia StreamVault",
    },
};
