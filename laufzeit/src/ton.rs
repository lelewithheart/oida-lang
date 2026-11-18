//! Töne (für `BRAUCH ton OIDA`): erzeugte Klänge wie beim Oszillator im Browser.
//!
//! Jeder Ton ist eine "Stimme": eine Schwingung mit Höhe, Dauer und
//! Lautstärke, die von selbst leiser wird und aufhört. Mehrere Stimmen
//! laufen gleichzeitig; [`mische`] rechnet sie zusammen.
//!
//! Ausgegeben wird über `waveOut` von Windows (winmm.dll). Ein eigener
//! Faden füllt laufend kleine Stücke nach. Er fasst nur die Liste der
//! Stimmen an (hinter einer Sperre), sonst nichts von der Laufzeit.
//!
//! Mit der Umgebungsvariable `OIDA_TON_AUS=1` bleibt alles stumm (für Tests).
//! Mit `OIDA_TON_PRUEFEN=1` meldet das Programm am Ende, wie viele Stücke
//! Windows angenommen hat: So lässt sich prüfen, dass wirklich etwas beim
//! Lautsprecher ankommt, ohne zuzuhören.

use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use super::{TextKopf, bytes};

/// So viele Werte pro Sekunde.
const RATE: f64 = 44100.0;
/// So viele Werte hat ein Stück, das auf einmal an Windows geht (rund 23 ms).
const STUECK: usize = 1024;
/// Mehr Töne gleichzeitig gibt es nicht; kommt einer dazu, fällt der älteste weg.
const HOECHSTENS: usize = 48;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Art {
    Sinus,
    Eckig,
    Saege,
    Dreieck,
    Rauschen,
}

struct Stimme {
    art: Art,
    /// Die Höhe am Anfang und am Ende, in Hertz. Dazwischen gleitet der Ton.
    von: f64,
    bis: f64,
    /// Wie viele Werte der Ton dauert und wie viele davon schon gespielt sind.
    dauer: usize,
    stelle: usize,
    /// So viele Werte lang wartet der Ton noch, bevor er anfängt.
    wartet: usize,
    laut: f32,
    /// Wo in der Schwingung der Ton gerade ist (0 bis 1).
    phase: f64,
    zufall: u32,
}

static STIMMEN: Mutex<Vec<Stimme>> = Mutex::new(Vec::new());
static GESTARTET: AtomicBool = AtomicBool::new(false);
/// Die Lautstärke für alles zusammen, als Bits einer f32 (damit sie ohne Sperre geht).
static GESAMT: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0
/// Wie viele Stücke Windows angenommen hat, und wie viele davon nicht still waren.
pub(crate) static ANGENOMMEN: AtomicU64 = AtomicU64::new(0);
pub(crate) static MIT_TON: AtomicU64 = AtomicU64::new(0);
/// Die Uhr der Töne: wie viele Werte schon gemischt sind.
static GEMISCHT: AtomicU64 = AtomicU64::new(0);

/// Rechnet alle Stimmen zusammen und schreibt das nächste Stück in `puffer`.
/// Stimmen, die zu Ende sind, fallen aus der Liste.
fn mische(stimmen: &mut Vec<Stimme>, puffer: &mut [i16], gesamt: f32) {
    let mut summe = vec![0f32; puffer.len()];
    for s in stimmen.iter_mut() {
        for wert in summe.iter_mut() {
            if s.wartet > 0 {
                s.wartet -= 1;
                continue;
            }
            if s.stelle >= s.dauer {
                break;
            }
            let anteil = s.stelle as f64 / s.dauer as f64;
            // Die Höhe gleitet so, wie das Ohr es als gleichmäßig hört (in Tonschritten, nicht in Hertz).
            let hoehe = if s.von > 0.0 && s.bis > 0.0 { s.von * (s.bis / s.von).powf(anteil) } else { s.von };
            let schwingung = match s.art {
                Art::Sinus => (s.phase * std::f64::consts::TAU).sin(),
                Art::Eckig => {
                    if s.phase < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                Art::Saege => 2.0 * s.phase - 1.0,
                Art::Dreieck => 1.0 - 4.0 * (s.phase - 0.5).abs(),
                Art::Rauschen => {
                    // Ein einfacher Zufallsgenerator (xorshift); gut genug für Rauschen.
                    s.zufall ^= s.zufall << 13;
                    s.zufall ^= s.zufall >> 17;
                    s.zufall ^= s.zufall << 5;
                    s.zufall as f64 / u32::MAX as f64 * 2.0 - 1.0
                }
            };
            // Ganz kurz einblenden (sonst knackt es), dann leiser bis zum Ende: am Anfang
            // schnell, dann immer langsamer, wie eine angeschlagene Saite (und wie
            // exponentialRampToValueAtTime im Browser, bis auf ein Tausendstel).
            let einblenden = (s.stelle as f64 / (0.002 * RATE)).min(1.0);
            let huelle = einblenden * 0.001f64.powf(anteil);
            *wert += (schwingung * huelle) as f32 * s.laut;
            s.phase = (s.phase + hoehe / RATE).fract();
            s.stelle += 1;
        }
    }
    stimmen.retain(|s| s.stelle < s.dauer);
    for (ziel, wert) in puffer.iter_mut().zip(summe) {
        // Zu laut wird sanft gedeckelt statt hart abgeschnitten.
        let begrenzt = (wert * gesamt).tanh();
        *ziel = (begrenzt * 32000.0) as i16;
    }
}

// --------------------------------------------------------- Ausgabe über Windows

type Zeiger = *mut c_void;

#[repr(C)]
struct TonFormat {
    art: u16,
    kanaele: u16,
    rate: u32,
    bytes_pro_sekunde: u32,
    block: u16,
    bits: u16,
    extra: u16,
}

#[repr(C)]
struct TonKopf {
    daten: *mut i16,
    laenge: u32,
    aufgenommen: u32,
    eigenes: usize,
    merker: u32,
    schleifen: u32,
    naechster: Zeiger,
    reserviert: usize,
}

unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> Zeiger;
    fn GetProcAddress(bibliothek: Zeiger, name: *const u8) -> Zeiger;
}

/// Sucht eine Funktion in winmm.dll (null, wenn es sie nicht gibt).
unsafe fn winmm(name: &[u8]) -> Zeiger {
    unsafe {
        let dll: Vec<u16> = "winmm.dll".encode_utf16().chain(std::iter::once(0)).collect();
        let b = LoadLibraryW(dll.as_ptr());
        if b.is_null() { std::ptr::null_mut() } else { GetProcAddress(b, name.as_ptr()) }
    }
}

/// Der Faden, der die Töne an Windows weitergibt. Läuft, bis das Programm aufhört.
fn ausgabe() {
    unsafe {
        let (auf, vorbereiten, schreiben) =
            (winmm(b"waveOutOpen\0"), winmm(b"waveOutPrepareHeader\0"), winmm(b"waveOutWrite\0"));
        if auf.is_null() || vorbereiten.is_null() || schreiben.is_null() {
            return;
        }
        let auf: unsafe extern "system" fn(*mut Zeiger, u32, *const TonFormat, usize, usize, u32) -> u32 =
            std::mem::transmute(auf);
        let vorbereiten: unsafe extern "system" fn(Zeiger, *mut TonKopf, u32) -> u32 = std::mem::transmute(vorbereiten);
        let schreiben: unsafe extern "system" fn(Zeiger, *mut TonKopf, u32) -> u32 = std::mem::transmute(schreiben);

        // Ein Kanal, 16 Bit, 44100 Werte pro Sekunde.
        let format = TonFormat {
            art: 1,
            kanaele: 1,
            rate: RATE as u32,
            bytes_pro_sekunde: RATE as u32 * 2,
            block: 2,
            bits: 16,
            extra: 0,
        };
        let mut geraet: Zeiger = std::ptr::null_mut();
        // u32::MAX: das Gerät, das in Windows als Standard eingestellt ist.
        if auf(&mut geraet, u32::MAX, &format, 0, 0, 0) != 0 {
            return; // kein Lautsprecher: Das Programm läuft ohne Ton weiter.
        }

        // Vier Stücke reihum: Zwei bis drei sind immer bei Windows in der Warteschlange.
        let mut daten: Vec<Vec<i16>> = (0..4).map(|_| vec![0i16; STUECK]).collect();
        let mut koepfe: Vec<TonKopf> = daten
            .iter_mut()
            .map(|d| TonKopf {
                daten: d.as_mut_ptr(),
                laenge: (STUECK * 2) as u32,
                aufgenommen: 0,
                eigenes: 0,
                merker: 0,
                schleifen: 0,
                naechster: std::ptr::null_mut(),
                reserviert: 0,
            })
            .collect();
        let kopf_groesse = std::mem::size_of::<TonKopf>() as u32;
        for kopf in koepfe.iter_mut() {
            vorbereiten(geraet, kopf, kopf_groesse);
        }
        // Merker von Windows: 1 = fertig gespielt, 16 = in der Warteschlange.
        const FERTIG: u32 = 1;
        const WARTET: u32 = 16;
        loop {
            for (i, kopf) in koepfe.iter_mut().enumerate() {
                let merker = std::ptr::read_volatile(&kopf.merker);
                if merker & WARTET != 0 && merker & FERTIG == 0 {
                    continue;
                }
                let gesamt = f32::from_bits(GESAMT.load(Ordering::Relaxed));
                if let Ok(mut stimmen) = STIMMEN.lock() {
                    mische(&mut stimmen, &mut daten[i], gesamt);
                    GEMISCHT.fetch_add(STUECK as u64, Ordering::Relaxed);
                }
                kopf.merker &= !FERTIG;
                if schreiben(geraet, kopf, kopf_groesse) == 0 {
                    ANGENOMMEN.fetch_add(1, Ordering::Relaxed);
                    if daten[i].iter().any(|w| *w != 0) {
                        MIT_TON.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
    }
}

fn art_von(name: &[u8]) -> Option<Art> {
    match String::from_utf8_lossy(name).to_lowercase().as_str() {
        "sinus" => Some(Art::Sinus),
        "eckig" => Some(Art::Eckig),
        "säge" | "saege" => Some(Art::Saege),
        "dreieck" => Some(Art::Dreieck),
        "rauschn" | "rauschen" => Some(Art::Rauschen),
        _ => None,
    }
}

/// Spielt einen Ton: Art der Schwingung, Höhe am Anfang und am Ende (Hertz),
/// Dauer (Sekunden) und Lautstärke (0 bis 1). Kommt sofort zurück; der Ton
/// läuft nebenher.
///
/// # Safety
/// `art` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_ton(art: *const TextKopf, von: f64, bis: f64, dauer: f64, laut: f64) {
    unsafe { neue_stimme(art, von, bis, dauer, laut, None) };
}

/// Wie [`oida_ton`], aber der Ton fängt erst an, wenn die Uhr der Töne
/// (`ton_zeit()`) bei `wann` steht. So lässt sich Musik auf den Punkt genau
/// vorausplanen, egal wie gleichmäßig das Programm selbst läuft.
///
/// # Safety
/// `art` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_ton_um(wann: f64, art: *const TextKopf, von: f64, bis: f64, dauer: f64, laut: f64) {
    unsafe { neue_stimme(art, von, bis, dauer, laut, Some(wann)) };
}

/// `ton_zeit()`: die Uhr der Töne in Sekunden. Sie läuft, sobald der erste Ton gespielt wurde.
#[unsafe(no_mangle)]
pub extern "C" fn oida_ton_zeit() -> f64 {
    GEMISCHT.load(Ordering::Relaxed) as f64 / RATE
}

unsafe fn neue_stimme(art: *const TextKopf, von: f64, bis: f64, dauer: f64, laut: f64, wann: Option<f64>) {
    let name = unsafe { bytes(art) };
    let Some(art) = art_von(name) else {
        super::leeren();
        eprintln!(
            "Heast! An Ton von da Oat \"{}\" gibt's ned. Es gibt: sinus, eckig, säge, dreieck, rauschn.",
            String::from_utf8_lossy(name)
        );
        std::process::exit(1);
    };
    if std::env::var_os("OIDA_TON_AUS").is_some_and(|w| !w.is_empty()) {
        return;
    }
    // Sinnlose Werte geben keinen Ton statt Krach.
    let dauer = (dauer.clamp(0.0, 30.0) * RATE) as usize;
    if dauer == 0 || !von.is_finite() || !bis.is_finite() {
        return;
    }
    let mut stimme = Stimme {
        art,
        von: von.clamp(0.0, 20000.0),
        bis: bis.clamp(0.0, 20000.0),
        dauer,
        stelle: 0,
        wartet: 0,
        laut: laut.clamp(0.0, 1.0) as f32,
        phase: 0.0,
        zufall: 0x9E37_79B9 ^ (dauer as u32).wrapping_mul(2654435761),
    };
    if let Ok(mut stimmen) = STIMMEN.lock() {
        // Unter der Sperre steht die Uhr still: Der Abstand zum Zeitpunkt stimmt auf den Wert genau.
        if let Some(wann) = wann.filter(|w| w.is_finite()) {
            let ziel = (wann.clamp(0.0, 1e9) * RATE) as u64;
            // Mehr als eine Minute im Voraus plant niemand; so ein Wert ist ein Versehen.
            stimme.wartet = ziel.saturating_sub(GEMISCHT.load(Ordering::Relaxed)).min(60 * RATE as u64) as usize;
        }
        if stimmen.len() >= HOECHSTENS {
            stimmen.remove(0);
        }
        stimmen.push(stimme);
    }
    // Beim ersten Ton den Faden starten, der an Windows liefert.
    if !GESTARTET.swap(true, Ordering::SeqCst) {
        let _ = std::thread::Builder::new().name("oida-ton".to_string()).spawn(ausgabe);
    }
}

/// `wart(sekunden)`: Das Programm macht so lange Pause (Töne laufen weiter).
#[unsafe(no_mangle)]
pub extern "C" fn oida_warten(sekunden: f64) {
    // Was bisher ausgegeben wurde, soll vor der Pause zu sehen sein.
    super::leeren();
    if sekunden.is_finite() && sekunden > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(sekunden.min(3600.0)));
    }
}

/// Die Lautstärke für alles zusammen: 0 ist stumm, 1 normal.
#[unsafe(no_mangle)]
pub extern "C" fn oida_ton_laut(wert: f64) {
    GESAMT.store((wert.clamp(0.0, 2.0) as f32).to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stimme(art: Art, von: f64, bis: f64, sekunden: f64, laut: f32) -> Stimme {
        Stimme { art, von, bis, dauer: (sekunden * RATE) as usize, stelle: 0, wartet: 0, laut, phase: 0.0, zufall: 12345 }
    }

    #[test]
    fn stille_ohne_stimmen() {
        let mut puffer = [7i16; 64];
        mische(&mut Vec::new(), &mut puffer, 1.0);
        assert!(puffer.iter().all(|w| *w == 0));
    }

    #[test]
    fn ein_ton_schwingt_und_hoert_auf() {
        // 441 Hertz: genau 100 Werte je Schwingung.
        let mut stimmen = vec![stimme(Art::Sinus, 441.0, 441.0, 0.05, 0.5)];
        let mut erstes = [0i16; 1024];
        mische(&mut stimmen, &mut erstes, 1.0);
        assert_eq!(stimmen.len(), 1);
        // Es schwingt um die Null herum, nach oben und nach unten.
        assert!(erstes.iter().any(|w| *w > 5000) && erstes.iter().any(|w| *w < -5000));
        // Vom höchsten Punkt bis zum nächsten sind es rund 100 Werte.
        let gipfel: Vec<usize> =
            (301..900).filter(|i| erstes[*i] > 0 && erstes[*i] >= erstes[i - 1] && erstes[*i] > erstes[i + 1]).collect();
        assert!((98..=102).contains(&(gipfel[1] - gipfel[0])), "{gipfel:?}");
        // Leiser bis zum Ende: Die zweite Hälfte ist schwächer als die erste.
        let staerke = |teil: &[i16]| teil.iter().map(|w| (*w as i64).abs()).sum::<i64>();
        assert!(staerke(&erstes[512..]) < staerke(&erstes[..512]));
        // 0,05 Sekunden sind 2205 Werte: Nach drei Stücken ist der Ton weg, danach ist es still.
        let mut naechstes = [0i16; 1024];
        mische(&mut stimmen, &mut naechstes, 1.0);
        mische(&mut stimmen, &mut naechstes, 1.0);
        assert!(stimmen.is_empty());
        assert!(naechstes[200..].iter().all(|w| *w == 0));
    }

    #[test]
    fn zwei_toene_gleichzeitig_und_nie_zu_laut() {
        let mut einer = vec![stimme(Art::Eckig, 200.0, 200.0, 1.0, 0.3)];
        let mut zwei = vec![stimme(Art::Eckig, 200.0, 200.0, 1.0, 0.3), stimme(Art::Eckig, 200.0, 200.0, 1.0, 0.3)];
        let (mut a, mut b) = ([0i16; 512], [0i16; 512]);
        mische(&mut einer, &mut a, 1.0);
        mische(&mut zwei, &mut b, 1.0);
        // Zwei gleiche Töne sind zusammen lauter als einer.
        assert!(b[400].abs() > a[400].abs());
        // Auch viele sehr laute Töne laufen nie über.
        let mut viele: Vec<Stimme> = (0..40).map(|_| stimme(Art::Eckig, 200.0, 200.0, 1.0, 1.0)).collect();
        let mut laut = [0i16; 512];
        mische(&mut viele, &mut laut, 2.0);
        assert!(laut.iter().all(|w| w.abs() <= 32000));
        // Mit Lautstärke 0 ist es still.
        mische(&mut zwei, &mut b, 0.0);
        assert!(b.iter().all(|w| *w == 0));
    }

    #[test]
    fn gleiten_und_rauschen() {
        // Von 200 auf 800 Hertz: Am Ende wechselt das Vorzeichen öfter als am Anfang.
        let mut stimmen = vec![stimme(Art::Saege, 200.0, 800.0, 0.2, 0.8)];
        let mut alles = vec![0i16; 8820];
        mische(&mut stimmen, &mut alles, 1.0);
        let wechsel = |teil: &[i16]| teil.windows(2).filter(|p| (p[0] < 0) != (p[1] < 0)).count();
        assert!(wechsel(&alles[6000..8000]) > wechsel(&alles[..2000]) * 2);
        let mut stimmen = vec![stimme(Art::Rauschen, 0.0, 0.0, 0.1, 0.5)];
        let mut rauschen = [0i16; 512];
        mische(&mut stimmen, &mut rauschen, 1.0);
        let verschieden: std::collections::HashSet<i16> = rauschen[300..].iter().copied().collect();
        assert!(verschieden.len() > 100);
    }

    #[test]
    fn ein_ton_kann_warten() {
        // Der Ton fängt erst nach 600 Werten an: davor ist es still, danach nicht.
        let mut stimmen = vec![Stimme { wartet: 600, ..stimme(Art::Eckig, 441.0, 441.0, 0.05, 0.5) }];
        let mut puffer = [0i16; 1024];
        mische(&mut stimmen, &mut puffer, 1.0);
        assert!(puffer[..600].iter().all(|w| *w == 0));
        assert!(puffer[700..].iter().any(|w| w.abs() > 3000));
        // Er dauert trotzdem seine ganze Länge (2205 Werte), nur später.
        mische(&mut stimmen, &mut puffer, 1.0);
        assert_eq!(stimmen.len(), 1);
        mische(&mut stimmen, &mut puffer, 1.0);
        assert!(stimmen.is_empty());
    }

    #[test]
    fn leiser_wie_im_browser() {
        // Nach der halben Dauer ist die Hülle bei Wurzel aus einem Tausendstel (rund 0,03).
        let mut stimmen = vec![stimme(Art::Eckig, 100.0, 100.0, 0.1, 1.0)];
        let mut puffer = vec![0i16; 4410];
        mische(&mut stimmen, &mut puffer, 1.0);
        let hoehe = |i: usize| (puffer[i] as f64 / 32000.0).atanh().abs();
        assert!((hoehe(2205) - 0.0316).abs() < 0.003, "{}", hoehe(2205));
        assert!(hoehe(300) > 0.5);
    }

    #[test]
    fn namen_der_arten() {
        assert_eq!(art_von("Sinus".as_bytes()), Some(Art::Sinus));
        assert_eq!(art_von("säge".as_bytes()), Some(Art::Saege));
        assert_eq!(art_von("rauschn".as_bytes()), Some(Art::Rauschen));
        assert_eq!(art_von(b"geige"), None);
        assert_eq!(std::mem::size_of::<TonKopf>(), 48);
    }
}
