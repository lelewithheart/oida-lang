//! Musik aus Dateien (MP3): laden, abspielen, im Kreis wiederholen, leiser
//! und lauter werden lassen. Mehrere Stücke können zugleich laufen; so
//! blendet man von einem ins andere über.
//!
//! Das Programm kennt ein Stück nur als Nummer. Geladen wird nebenher in
//! einem eigenen Faden, damit das Programm dabei nicht stockt; gespielt wird
//! vom Faden der Töne, der die Stücke zu den erzeugten Tönen dazumischt.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use super::{GEMISCHT, RATE, TAKT, TextKopf, bytes, mp3};

enum Zustand {
    Laedt,
    /// Die Töne: 44100 Werte pro Sekunde, links und rechts abwechselnd.
    Fertig(Arc<Vec<i16>>),
    Fehler(String),
}

struct Stueck {
    zustand: Zustand,
    /// Unterscheidet dieses Stück von einem früheren mit derselben Nummer.
    marke: u64,
    spielt: bool,
    /// Wo es gerade ist (in Werten pro Kanal) und wie lang es noch wartet, bevor es anfängt.
    stelle: usize,
    wartet: usize,
    /// Wiederholen: von wo bis wo (in Werten pro Kanal).
    schleife: Option<(usize, usize)>,
    /// Die Lautstärke jetzt, wohin sie soll und um wie viel sie sich pro Wert ändert.
    laut: f32,
    ziel_laut: f32,
    schritt: f32,
    /// Ist die Lautstärke bei 0 angekommen, hört das Stück auf.
    dann_aus: bool,
}

static STUECKE: Mutex<Vec<Option<Stueck>>> = Mutex::new(Vec::new());
static NAECHSTE_MARKE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Mischt alle laufenden Stücke zu `summe` dazu (links, rechts, links, rechts ...).
pub(super) fn dazumischen(summe: &mut [f32]) {
    let Ok(mut stuecke) = STUECKE.lock() else { return };
    for stueck in stuecke.iter_mut().flatten() {
        if !stueck.spielt {
            continue;
        }
        let Zustand::Fertig(daten) = &stueck.zustand else { continue };
        let daten = Arc::clone(daten);
        let laenge = daten.len() / 2;
        for paar in summe.chunks_exact_mut(2) {
            if stueck.wartet > 0 {
                stueck.wartet -= 1;
                continue;
            }
            if let Some((von, bis)) = stueck.schleife
                && stueck.stelle >= bis
            {
                stueck.stelle = von + (stueck.stelle - bis);
            }
            if stueck.stelle >= laenge {
                stueck.spielt = false;
                break;
            }
            if stueck.schritt != 0.0 {
                stueck.laut += stueck.schritt;
                let da = if stueck.schritt > 0.0 { stueck.laut >= stueck.ziel_laut } else { stueck.laut <= stueck.ziel_laut };
                if da {
                    stueck.laut = stueck.ziel_laut;
                    stueck.schritt = 0.0;
                    if stueck.dann_aus {
                        stueck.spielt = false;
                        break;
                    }
                }
            }
            paar[0] += daten[stueck.stelle * 2] as f32 / 32768.0 * stueck.laut;
            paar[1] += daten[stueck.stelle * 2 + 1] as f32 / 32768.0 * stueck.laut;
            stueck.stelle += 1;
        }
    }
}

fn mit<T>(nummer: i64, sonst: T, tu: impl FnOnce(&mut Stueck) -> T) -> T {
    let Ok(mut stuecke) = STUECKE.lock() else { return sonst };
    match stuecke.get_mut(nummer.max(0) as usize).and_then(|platz| platz.as_mut()) {
        Some(stueck) if nummer >= 0 => tu(stueck),
        _ => {
            drop(stuecke);
            crate::leeren();
            eprintln!("Heast! A Musikstickl mit da Numma {nummer} gibt's ned (oda nimma).");
            std::process::exit(1);
        }
    }
}

/// `musik_lodn(pfad)`: fängt an, die Datei zu laden, und gibt gleich eine Nummer zurück.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_musik_laden(pfad: *const TextKopf) -> i64 {
    let pfad = String::from_utf8_lossy(unsafe { bytes(pfad) }).into_owned();
    let marke = NAECHSTE_MARKE.fetch_add(1, Ordering::Relaxed);
    let neu = Stueck {
        zustand: Zustand::Laedt,
        marke,
        spielt: false,
        stelle: 0,
        wartet: 0,
        schleife: None,
        laut: 1.0,
        ziel_laut: 1.0,
        schritt: 0.0,
        dann_aus: false,
    };
    let nummer = {
        let Ok(mut stuecke) = STUECKE.lock() else { return -1 };
        match stuecke.iter().position(|platz| platz.is_none()) {
            Some(frei) => {
                stuecke[frei] = Some(neu);
                frei
            }
            None => {
                stuecke.push(Some(neu));
                stuecke.len() - 1
            }
        }
    };
    let laden = move || {
        let ergebnis = match std::fs::read(&pfad) {
            Ok(daten) => mp3::entpacken(&daten),
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => Err("De gibt's ned.".to_string()),
            Err(fehler) => Err(fehler.to_string()),
        };
        let Ok(mut stuecke) = STUECKE.lock() else { return };
        // Nur eintragen, wenn es das Stück noch gibt (und an der Stelle kein anderes steht).
        if let Some(Some(stueck)) = stuecke.get_mut(nummer)
            && stueck.marke == marke
        {
            stueck.zustand = match ergebnis {
                Ok(daten) => Zustand::Fertig(Arc::new(daten)),
                Err(grund) => Zustand::Fehler(grund),
            };
        }
    };
    if std::thread::Builder::new().name("oida-musik".to_string()).spawn(laden).is_err() {
        return -1;
    }
    nummer as i64
}

/// `musik_zuastand(numma)`: 0 lädt noch, 1 ist da, -1 hat nicht geklappt.
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_zustand(nummer: i64) -> i64 {
    mit(nummer, -1, |s| match s.zustand {
        Zustand::Laedt => 0,
        Zustand::Fertig(_) => 1,
        Zustand::Fehler(_) => -1,
    })
}

/// Warum das Laden nicht geklappt hat (sonst ein leerer Text).
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_fehler(nummer: i64) -> *mut TextKopf {
    let grund = mit(nummer, String::new(), |s| match &s.zustand {
        Zustand::Fehler(grund) => grund.clone(),
        _ => String::new(),
    });
    crate::neuer_text(grund.as_bytes())
}

/// `musik_läng(numma)`: wie lang das Stück ist, in Sekunden (0, solange es lädt).
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_laenge(nummer: i64) -> f64 {
    mit(nummer, 0.0, |s| match &s.zustand {
        Zustand::Fertig(daten) => (daten.len() / 2) as f64 / RATE,
        _ => 0.0,
    })
}

/// Spielt ein Stück von vorne: mit dieser Lautstärke, ab dem Zeitpunkt `wann`
/// auf der Uhr der Töne (kleiner als 0: sofort), und wenn `bis` größer ist als
/// `von`, wiederholt es danach das Stück dazwischen ohne Ende.
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_spielen(nummer: i64, laut: f64, wann: f64, von: f64, bis: f64) {
    let stumm = std::env::var_os("OIDA_TON_AUS").is_some_and(|w| !w.is_empty());
    // Unter dieser Sperre steht die Uhr der Töne still.
    let takt = TAKT.lock();
    mit(nummer, (), |s| {
        s.spielt = true;
        s.stelle = 0;
        s.laut = laut.clamp(0.0, 4.0) as f32;
        s.ziel_laut = s.laut;
        s.schritt = 0.0;
        s.dann_aus = false;
        s.schleife = (bis > von && von >= 0.0).then(|| ((von * RATE) as usize, (bis * RATE) as usize));
        s.wartet = if wann >= 0.0 && wann.is_finite() {
            ((wann.min(1e9) * RATE) as u64).saturating_sub(GEMISCHT.load(Ordering::Relaxed)).min(600 * RATE as u64) as usize
        } else {
            0
        };
    });
    drop(takt);
    if !stumm {
        super::faden_starten();
    }
}

/// `musik_laut(numma, laut, sekundn)`: Die Lautstärke geht in dieser Zeit auf den neuen Wert (0 Sekunden: sofort).
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_laut(nummer: i64, laut: f64, sekunden: f64) {
    mit(nummer, (), |s| {
        s.ziel_laut = laut.clamp(0.0, 4.0) as f32;
        s.dann_aus = false;
        if sekunden > 0.0 && s.spielt {
            s.schritt = (s.ziel_laut - s.laut) / (sekunden * RATE) as f32;
        } else {
            s.laut = s.ziel_laut;
            s.schritt = 0.0;
        }
    });
}

/// `musik_stopp(numma, sekundn)`: Das Stück wird in dieser Zeit leiser und hört dann auf (0 Sekunden: sofort).
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_stopp(nummer: i64, sekunden: f64) {
    mit(nummer, (), |s| {
        if sekunden > 0.0 && s.spielt && s.laut > 0.0 {
            s.ziel_laut = 0.0;
            s.schritt = -s.laut / (sekunden * RATE) as f32;
            s.dann_aus = true;
        } else {
            s.spielt = false;
        }
    });
}

/// `musik_rennt(numma)`: Läuft das Stück gerade (oder wartet es auf seinen Einsatz)?
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_laeuft(nummer: i64) -> bool {
    mit(nummer, false, |s| s.spielt)
}

/// `musik_stö(numma)`: wo im Stück es gerade ist, in Sekunden.
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_stelle(nummer: i64) -> f64 {
    mit(nummer, 0.0, |s| s.stelle as f64 / RATE)
}

/// `musik_weg(numma)`: gibt das Stück her. Seine Nummer gilt danach nicht mehr.
#[unsafe(no_mangle)]
pub extern "C" fn oida_musik_weg(nummer: i64) {
    mit(nummer, (), |_| ());
    if let Ok(mut stuecke) = STUECKE.lock() {
        stuecke[nummer as usize] = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legt ein Stück mit fertigen Tönen an (ohne Datei) und gibt seine Nummer zurück.
    fn stueck(daten: Vec<i16>) -> i64 {
        let mut stuecke = STUECKE.lock().unwrap();
        stuecke.push(Some(Stueck {
            zustand: Zustand::Fertig(Arc::new(daten)),
            marke: 0,
            spielt: false,
            stelle: 0,
            wartet: 0,
            schleife: None,
            laut: 1.0,
            ziel_laut: 1.0,
            schritt: 0.0,
            dann_aus: false,
        }));
        stuecke.len() as i64 - 1
    }

    /// Mischt nur dieses eine Stück (die anderen Tests haben eigene).
    fn spiel(nummer: i64, werte: usize) -> Vec<f32> {
        let mut summe = vec![0f32; werte * 2];
        let mut stuecke = STUECKE.lock().unwrap();
        let andere: Vec<bool> = stuecke.iter().map(|s| s.as_ref().is_some_and(|s| s.spielt)).collect();
        for (i, s) in stuecke.iter_mut().enumerate() {
            if let Some(s) = s
                && i as i64 != nummer
            {
                s.spielt = false;
            }
        }
        drop(stuecke);
        dazumischen(&mut summe);
        let mut stuecke = STUECKE.lock().unwrap();
        for (i, s) in stuecke.iter_mut().enumerate() {
            if let Some(s) = s
                && i as i64 != nummer
            {
                s.spielt = andere[i];
            }
        }
        summe
    }

    // Die Tests teilen sich die eine Liste der Stücke: nacheinander laufen lassen.
    static EINER: Mutex<()> = Mutex::new(());

    #[test]
    fn spielt_einmal_durch_links_und_rechts() {
        let _wache = EINER.lock().unwrap_or_else(|e| e.into_inner());
        // Vier Werte: links steigt, rechts ist das Negative davon.
        let n = stueck(vec![8192, -8192, 16384, -16384, 8192, -8192, 0, 0]);
        oida_musik_laut(n, 0.5, 0.0);
        mit(n, (), |s| s.spielt = true);
        let aus = spiel(n, 6);
        assert_eq!(&aus[..8], &[0.125, -0.125, 0.25, -0.25, 0.125, -0.125, 0.0, 0.0]);
        // Danach ist es zu Ende, und es kommt nichts mehr.
        assert!(!oida_musik_laeuft(n));
        assert_eq!(&aus[8..], &[0.0; 4]);
        assert_eq!(oida_musik_laenge(n), 4.0 / RATE);
        oida_musik_weg(n);
    }

    #[test]
    fn wiederholt_ein_stueck_im_kreis() {
        let _wache = EINER.lock().unwrap_or_else(|e| e.into_inner());
        // Werte 1 bis 5 (mal 3276.8, damit glatte Zahlen herauskommen); wiederholt wird von 1 bis 4.
        let daten: Vec<i16> = (1..=5).flat_map(|w| [w * 3277, w * 3277]).collect();
        let n = stueck(daten);
        mit(n, (), |s| {
            s.spielt = true;
            s.schleife = Some((1, 4));
        });
        let aus = spiel(n, 10);
        let links: Vec<i32> = aus.chunks(2).map(|p| (p[0] * 10.0).round() as i32).collect();
        // Vorlauf 1, dann 2 3 4 immer wieder; die 5 kommt nie.
        assert_eq!(links, [1, 2, 3, 4, 2, 3, 4, 2, 3, 4]);
        assert!(oida_musik_laeuft(n));
        oida_musik_weg(n);
    }

    #[test]
    fn wird_leiser_und_hoert_auf() {
        let _wache = EINER.lock().unwrap_or_else(|e| e.into_inner());
        let n = stueck(vec![16384; 2000]);
        mit(n, (), |s| s.spielt = true);
        // In 441 Werten (einer Hundertstelsekunde) von 1 auf 0.
        oida_musik_stopp(n, 0.01);
        let aus = spiel(n, 600);
        assert!((aus[0] - 0.5).abs() < 0.01);
        assert!((aus[2 * 220] - 0.25).abs() < 0.01, "{}", aus[2 * 220]);
        assert!(aus[2 * 450..].iter().all(|w| *w == 0.0));
        assert!(!oida_musik_laeuft(n));
        // Wieder von vorne, und lauter werden lassen.
        mit(n, (), |s| {
            s.spielt = true;
            s.stelle = 0;
            s.laut = 0.0;
        });
        oida_musik_laut(n, 1.0, 0.01);
        let aus = spiel(n, 600);
        assert!(aus[0] < 0.01 && (aus[2 * 220] - 0.25).abs() < 0.01 && (aus[2 * 500] - 0.5).abs() < 0.001);
        assert!(oida_musik_laeuft(n));
        oida_musik_weg(n);
    }

    #[test]
    fn wartet_auf_den_einsatz() {
        let _wache = EINER.lock().unwrap_or_else(|e| e.into_inner());
        let n = stueck(vec![16384; 200]);
        mit(n, (), |s| {
            s.spielt = true;
            s.wartet = 30;
        });
        let aus = spiel(n, 50);
        assert!(aus[..60].iter().all(|w| *w == 0.0));
        assert!(aus[60..].iter().all(|w| *w == 0.5));
        assert_eq!(oida_musik_stelle(n), 20.0 / RATE);
        oida_musik_weg(n);
    }
}
