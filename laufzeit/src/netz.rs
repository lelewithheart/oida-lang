//! Netzwerk (für `BRAUCH netz OIDA`): TCP-Verbindungen in beide Richtungen
//! und HTTP samt HTTPS als Client.
//!
//! TCP kommt aus der Standardbibliothek von Rust und ist auf allen Systemen
//! dasselbe. Für HTTP gibt es zwei Unterbauten mit derselben Aufgabe:
//!
//! - Unter Windows macht alles WinHTTP (gehört zu Windows): Verbindung,
//!   Verschlüsselung, Umleitungen, Proxy.
//! - Sonst spricht OIDA HTTP/1.1 selbst ([`eigen`]). Für HTTPS nimmt es die
//!   OpenSSL-Bibliothek des Systems (`libssl`).
//!
//! Beides wird erst geladen, wenn ein Programm es wirklich braucht.

use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::dateien::{merke_fehler, merke_meldung};
use super::{ListenKopf, TextKopf, bytes, liste_aus_texten, liste_aus_zahlen, neuer_text};

/// Eine offene Verbindung: zum Lesen mit Puffer (für Zeilen), zum Schreiben direkt.
struct Verbindung {
    lesen: BufReader<TcpStream>,
    schreiben: TcpStream,
}

/// Was von der letzten HTTP-Anfrage zurückkam.
struct Antwort {
    code: i64,
    /// Die Köpfe, die Namen klein geschrieben, in der Reihenfolge, in der sie kamen.
    koepfe: Vec<(String, String)>,
    inhalt: Vec<u8>,
}

thread_local! {
    // Die Nummern, die ein Programm in der Hand hat, sind Stellen in diesen Listen.
    // Eine Stelle wird nie wiederverwendet: Wer nach dem Zumachen noch schreibt,
    // erwischt so nie die Verbindung von jemand anderem.
    static VERBINDUNGEN: RefCell<Vec<Option<Verbindung>>> = const { RefCell::new(Vec::new()) };
    static HORCHER: RefCell<Vec<Option<TcpListener>>> = const { RefCell::new(Vec::new()) };
    /// Wie lange auf das Netz gewartet wird, in Sekunden (0: ohne Ende).
    static WARTEZEIT: Cell<u64> = const { Cell::new(30) };
    static ANTWORT: RefCell<Antwort> = const { RefCell::new(Antwort { code: 0, koepfe: Vec::new(), inhalt: Vec::new() }) };
}

fn wartezeit() -> Option<Duration> {
    match WARTEZEIT.get() {
        0 => None,
        sekunden => Some(Duration::from_secs(sekunden)),
    }
}

/// # Safety
/// `text` muss ein gültiger Text sein.
unsafe fn text_von(text: *const TextKopf) -> String {
    String::from_utf8_lossy(unsafe { bytes(text) }).into_owned()
}

/// Merkt sich, was beim Reden über eine Verbindung schiefgegangen ist.
fn merke_netz(fehler: &std::io::Error) {
    use std::io::ErrorKind::*;
    match fehler.kind() {
        TimedOut | WouldBlock => merke_meldung("Do kummt nix mehr: z'laung gwort."),
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof => merke_meldung("De Vabindung is ogrissn."),
        ConnectionRefused => merke_meldung("Durt nimmt kana o."),
        AddrInUse => merke_meldung("Der Port is scho bsetzt."),
        PermissionDenied => merke_meldung("Den Port derf i ned nehma."),
        _ => merke_fehler(fehler),
    }
}

/// `netz_wortzeit(sekundn)`: wie lange auf eine Verbindung oder Antwort gewartet wird (0: ohne Ende).
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_wartezeit(sekunden: i64) {
    WARTEZEIT.set(sekunden.clamp(0, 86_400) as u64);
}

// ---------------------------------------------------------------------- TCP

/// Baut eine Verbindung zu einem Rechner auf; alle seine Adressen werden der Reihe nach versucht.
fn verbinde(host: &str, port: u16) -> Result<TcpStream, String> {
    let adressen: Vec<_> = (host, port).to_socket_addrs().map_err(|_| meldung::unbekannt(host))?.collect();
    let mut letzter = meldung::unbekannt(host);
    for adresse in adressen {
        let versuch = match wartezeit() {
            Some(zeit) => TcpStream::connect_timeout(&adresse, zeit),
            None => TcpStream::connect(adresse),
        };
        match versuch {
            Ok(strom) => {
                let _ = strom.set_read_timeout(wartezeit());
                let _ = strom.set_write_timeout(wartezeit());
                let _ = strom.set_nodelay(true);
                return Ok(strom);
            }
            Err(fehler) => {
                use std::io::ErrorKind::*;
                letzter = match fehler.kind() {
                    TimedOut | WouldBlock => meldung::stumm(host),
                    ConnectionRefused => format!("{host} nimmt auf Port {port} kane Vabindungen au."),
                    _ => format!("Zu {host} kumm i ned duach: {fehler}"),
                };
            }
        }
    }
    Err(letzter)
}

fn verbindung_ablegen(strom: TcpStream) -> i64 {
    let Ok(lesen) = strom.try_clone() else {
        merke_meldung("De Vabindung losst si ned aufmochn.");
        return -1;
    };
    VERBINDUNGEN.with(|v| {
        let mut v = v.borrow_mut();
        v.push(Some(Verbindung { lesen: BufReader::new(lesen), schreiben: strom }));
        v.len() as i64 - 1
    })
}

/// Tut etwas mit einer offenen Verbindung. Gibt es sie nicht (mehr) oder geht es schief,
/// kommt `None` zurück, und der Fehler ist gemerkt.
fn mit_verbindung<R>(nummer: i64, tu: impl FnOnce(&mut Verbindung) -> std::io::Result<R>) -> Option<R> {
    VERBINDUNGEN.with(|v| {
        let mut v = v.borrow_mut();
        let Some(verbindung) = usize::try_from(nummer).ok().and_then(|n| v.get_mut(n)).and_then(Option::as_mut) else {
            merke_meldung("De Vabindung is scho zua.");
            return None;
        };
        match tu(verbindung) {
            Ok(wert) => Some(wert),
            Err(fehler) => {
                merke_netz(&fehler);
                None
            }
        }
    })
}

/// `vabind(rechna, port)`: die Nummer der neuen Verbindung, oder -1.
///
/// # Safety
/// `host` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_netz_verbinden(host: *const TextKopf, port: i64) -> i64 {
    let host = unsafe { text_von(host) };
    let Ok(port) = u16::try_from(port) else {
        merke_meldung("An Port gibt's nur von 0 bis 65535.");
        return -1;
    };
    match verbinde(&host, port) {
        Ok(strom) => verbindung_ablegen(strom),
        Err(meldung) => {
            merke_meldung(&meldung);
            -1
        }
    }
}

/// Schickt einen Text über die Verbindung.
///
/// # Safety
/// `text` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_netz_schicken(nummer: i64, text: *const TextKopf) -> bool {
    let inhalt = unsafe { bytes(text) };
    mit_verbindung(nummer, |v| v.schreiben.write_all(inhalt).and_then(|()| v.schreiben.flush())).is_some()
}

/// Schickt Bytes (Zahlen von 0 bis 255) über die Verbindung.
///
/// # Safety
/// `liste` muss eine gültige Liste von ganzen Zahlen sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_netz_bytes_schicken(nummer: i64, liste: *const ListenKopf) -> bool {
    let zahlen = unsafe { std::slice::from_raw_parts((*liste).daten as *const i64, (*liste).laenge as usize) };
    if let Some(stelle) = zahlen.iter().position(|z| !(0..=255).contains(z)) {
        merke_meldung(&format!("An da Stö {stelle} steht {}, a Byte geht oba nur vo 0 bis 255.", zahlen[stelle]));
        return false;
    }
    let roh: Vec<u8> = zahlen.iter().map(|z| *z as u8).collect();
    mit_verbindung(nummer, |v| v.schreiben.write_all(&roh).and_then(|()| v.schreiben.flush())).is_some()
}

/// Die nächste Zeile (ohne ihr Ende), oder null, wenn die andere Seite zugemacht hat
/// oder nichts mehr kommt.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_zeile(nummer: i64) -> *mut TextKopf {
    let zeile = mit_verbindung(nummer, |v| {
        let mut zeile = Vec::new();
        v.lesen.read_until(b'\n', &mut zeile)?;
        Ok(zeile)
    });
    match zeile {
        Some(mut zeile) if !zeile.is_empty() => {
            if zeile.last() == Some(&b'\n') {
                zeile.pop();
                if zeile.last() == Some(&b'\r') {
                    zeile.pop();
                }
            }
            neuer_text(String::from_utf8_lossy(&zeile).as_bytes())
        }
        _ => std::ptr::null_mut(),
    }
}

/// Alles, was noch kommt, bis die andere Seite zumacht.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_alles_lesen(nummer: i64) -> *mut TextKopf {
    let alles = mit_verbindung(nummer, |v| {
        let mut alles = Vec::new();
        v.lesen.read_to_end(&mut alles)?;
        Ok(alles)
    });
    match alles {
        Some(alles) => neuer_text(String::from_utf8_lossy(&alles).as_bytes()),
        None => std::ptr::null_mut(),
    }
}

/// Bis zu `hoechstens` Bytes, sobald welche da sind. Eine leere Liste heißt: Da kommt nichts mehr.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_bytes_lesen(nummer: i64, hoechstens: i64) -> *mut ListenKopf {
    let gelesen = mit_verbindung(nummer, |v| {
        let mut puffer = vec![0u8; hoechstens.clamp(1, 1 << 20) as usize];
        let anzahl = v.lesen.read(&mut puffer)?;
        puffer.truncate(anzahl);
        Ok(puffer)
    });
    let zahlen: Vec<i64> = gelesen.unwrap_or_default().into_iter().map(i64::from).collect();
    liste_aus_zahlen(&zahlen)
}

/// Mit wem die Verbindung besteht: "127.0.0.1:51234".
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_gegenueber(nummer: i64) -> *mut TextKopf {
    let wer = mit_verbindung(nummer, |v| v.schreiben.peer_addr()).map(|a| a.to_string()).unwrap_or_default();
    neuer_text(wer.as_bytes())
}

/// Macht die Verbindung zu. Die andere Seite sieht dann beim Lesen das Ende.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_zu(nummer: i64) {
    VERBINDUNGEN.with(|v| {
        if let Some(platz) = usize::try_from(nummer).ok().and_then(|n| v.borrow_mut().get_mut(n).map(Option::take))
            && let Some(verbindung) = platz
        {
            let _ = verbindung.schreiben.shutdown(Shutdown::Both);
        }
    });
}

/// `horch(port)`: wartet auf Verbindungen. Mit `alle` auch von anderen Rechnern,
/// sonst nur von diesem. Port 0 heißt: irgendeinen freien. Gibt die Nummer zurück oder -1.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_horchen(port: i64, alle: bool) -> i64 {
    let Ok(port) = u16::try_from(port) else {
        merke_meldung("An Port gibt's nur von 0 bis 65535.");
        return -1;
    };
    match TcpListener::bind((if alle { "0.0.0.0" } else { "127.0.0.1" }, port)) {
        Ok(horcher) => HORCHER.with(|h| {
            let mut h = h.borrow_mut();
            h.push(Some(horcher));
            h.len() as i64 - 1
        }),
        Err(fehler) => {
            merke_netz(&fehler);
            -1
        }
    }
}

fn mit_horcher<R>(nummer: i64, tu: impl FnOnce(&TcpListener) -> std::io::Result<R>) -> Option<R> {
    HORCHER.with(|h| {
        let h = h.borrow();
        let Some(horcher) = usize::try_from(nummer).ok().and_then(|n| h.get(n)).and_then(Option::as_ref) else {
            merke_meldung("Der Horcha is scho zua.");
            return None;
        };
        match tu(horcher) {
            Ok(wert) => Some(wert),
            Err(fehler) => {
                merke_netz(&fehler);
                None
            }
        }
    })
}

/// Auf welchem Port gehorcht wird (wichtig nach `horch(0)`), oder -1.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_horcher_port(nummer: i64) -> i64 {
    mit_horcher(nummer, TcpListener::local_addr).map_or(-1, |a| i64::from(a.port()))
}

/// Wartet, bis sich jemand verbindet, und gibt die Nummer der Verbindung zurück (oder -1).
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_annehmen(nummer: i64) -> i64 {
    super::leeren();
    match mit_horcher(nummer, |h| h.accept().map(|(strom, _)| strom)) {
        Some(strom) => {
            let _ = strom.set_read_timeout(wartezeit());
            let _ = strom.set_write_timeout(wartezeit());
            let _ = strom.set_nodelay(true);
            verbindung_ablegen(strom)
        }
        None => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_horcher_zu(nummer: i64) {
    HORCHER.with(|h| {
        if let Some(platz) = usize::try_from(nummer).ok().and_then(|n| h.borrow_mut().get_mut(n).map(Option::take)) {
            drop(platz);
        }
    });
}

// --------------------------------------------------------------------- HTTP

/// Eine Adresse wie `https://rechner:8443/pfad?frage`, in ihre Teile zerlegt.
#[derive(Debug, Clone, PartialEq)]
struct Adresse {
    sicher: bool,
    host: String,
    port: u16,
    /// Was hinter dem Rechner steht, mit `/` am Anfang und so, wie es über die Leitung geht.
    pfad: String,
}

impl Adresse {
    fn zerlegen(adresse: &str) -> Result<Adresse, String> {
        let adresse = adresse.trim();
        let klein = adresse.to_ascii_lowercase();
        let (sicher, rest) = if klein.starts_with("https://") {
            (true, &adresse[8..])
        } else if klein.starts_with("http://") {
            (false, &adresse[7..])
        } else {
            return Err(format!("De Adress '{adresse}' muass mit http:// oda https:// aufaunga."));
        };
        // Was hinter # steht, gehört nur dem Browser.
        let rest = rest.split('#').next().unwrap_or("");
        let (wer, pfad) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));
        if wer.contains('@') {
            return Err("Nom und Passwort in da Adress (vor'm @) gehn ned. Gib's ois Kopf mit: Authorization.".to_string());
        }
        // rechner, rechner:port, [::1] oder [::1]:port
        let (host, port) = match wer.strip_prefix('[').and_then(|innen| innen.split_once(']')) {
            Some((host, danach)) => (host, danach.strip_prefix(':')),
            None => match wer.rsplit_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (wer, None),
            },
        };
        let port = match port {
            None | Some("") => {
                if sicher {
                    443
                } else {
                    80
                }
            }
            Some(port) => port
                .parse::<u16>()
                .ok()
                .filter(|p| *p != 0)
                .ok_or(format!("Der Port in '{adresse}' is ka Zoi von 1 bis 65535."))?,
        };
        if host.is_empty() || !host.is_ascii() || host.contains(|c: char| c.is_whitespace() || c == '%') {
            return Err(format!(
                "In da Adress '{adresse}' fehlt da Rechner, oda sei Nom hot Zeichn, de ned gehn (Umlaute, Leerzeichn)."
            ));
        }
        let pfad = match pfad {
            "" => "/".to_string(),
            frage if frage.starts_with('?') => format!("/{frage}"),
            pfad => pfad.to_string(),
        };
        // Leerzeichen, Umlaute und Steuerzeichen gehen nur verpackt über die Leitung (%20, %C3%A4).
        let mut verpackt = String::with_capacity(pfad.len());
        for byte in pfad.bytes() {
            if byte <= 0x20 || byte >= 0x7F {
                verpackt.push_str(&format!("%{byte:02X}"));
            } else {
                verpackt.push(byte as char);
            }
        }
        Ok(Adresse { sicher, host: host.to_ascii_lowercase(), port, pfad: verpackt })
    }
}

/// Die Köpfe einer Anfrage aus Zeilen der Form "Nom: Wert".
fn koepfe_lesen(zeilen: &[String]) -> Result<Vec<(String, String)>, String> {
    zeilen
        .iter()
        .map(|zeile| {
            let kaputt = || format!("'{zeile}' is ka Kopf. So ghört's: \"Nom: Wert\".");
            let (name, wert) = zeile.split_once(':').ok_or_else(kaputt)?;
            let (name, wert) = (name.trim(), wert.trim());
            let name_passt = !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            // Ein Zeilenende im Wert würde der anderen Seite einen zweiten Kopf unterschieben.
            if !name_passt || wert.contains(['\r', '\n']) {
                return Err(kaputt());
            }
            Ok((name.to_string(), wert.to_string()))
        })
        .collect()
}

/// Eine ganze Anfrage: schicken, Umleitungen folgen, Antwort einsammeln.
fn anfrage(methode: &str, adresse: &str, koepfe: &[String], inhalt: &[u8]) -> Result<Antwort, String> {
    let methode = methode.trim().to_ascii_uppercase();
    if methode.is_empty() || !methode.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(format!("'{methode}' is ka Oat vo Aufrog. Es gibt GET, POST, PUT, DELETE und a poa aundare."));
    }
    let adresse = Adresse::zerlegen(adresse)?;
    let koepfe = koepfe_lesen(koepfe)?;
    #[cfg(windows)]
    return windows::anfrage(&methode, &adresse, &koepfe, inhalt);
    #[cfg(not(windows))]
    return eigen::anfrage(&methode, adresse, &koepfe, inhalt);
}

/// Die Meldungen, die auf allen Systemen gleich lauten sollen.
mod meldung {
    pub fn unbekannt(host: &str) -> String {
        format!("Den Rechner {host} find i ned.")
    }
    pub fn stumm(host: &str) -> String {
        format!("{host} gibt ka Antwort.")
    }
    pub fn zertifikat(host: &str, grund: &str) -> String {
        if grund.is_empty() {
            format!("Dem Zertifikat vo {host} trau i ned.")
        } else {
            format!("Dem Zertifikat vo {host} trau i ned ({grund}).")
        }
    }
    pub fn abgerissen(host: &str) -> String {
        format!("De Vabindung mit {host} is ogrissn.")
    }
}

/// `aufrog(oat, adress, köpf, inhalt)`: Danach liegt die Antwort bereit (siehe die Funktionen darunter).
///
/// # Safety
/// `methode`, `adresse` und `inhalt` müssen gültige Texte sein, `koepfe` eine gültige Liste von Texten.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_netz_anfrage(
    methode: *const TextKopf,
    adresse: *const TextKopf,
    koepfe: *const ListenKopf,
    inhalt: *const TextKopf,
) -> bool {
    let (methode, adresse, inhalt) = unsafe { (text_von(methode), text_von(adresse), bytes(inhalt).to_vec()) };
    let koepfe: Vec<String> = unsafe {
        let l = &*koepfe;
        std::slice::from_raw_parts(l.daten as *const *const TextKopf, l.laenge as usize).iter().map(|t| text_von(*t)).collect()
    };
    super::leeren();
    match anfrage(&methode, &adresse, &koepfe, &inhalt) {
        Ok(antwort) => {
            ANTWORT.with(|a| *a.borrow_mut() = antwort);
            true
        }
        Err(meldung) => {
            merke_meldung(&meldung);
            false
        }
    }
}

/// Der Code der letzten Antwort: 200 heißt gut, 404 gibt es nicht, 500 der Server hat ein Problem.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_antwort_code() -> i64 {
    ANTWORT.with(|a| a.borrow().code)
}

/// Der Inhalt der letzten Antwort als Text.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_antwort_inhalt() -> *mut TextKopf {
    ANTWORT.with(|a| neuer_text(String::from_utf8_lossy(&a.borrow().inhalt).as_bytes()))
}

/// Der Inhalt der letzten Antwort als Bytes (für Bilder und alles, was kein Text ist).
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_antwort_bytes() -> *mut ListenKopf {
    ANTWORT.with(|a| liste_aus_zahlen(&a.borrow().inhalt.iter().map(|b| i64::from(*b)).collect::<Vec<i64>>()))
}

/// Die Köpfe der letzten Antwort: abwechselnd Name (klein geschrieben) und Wert.
#[unsafe(no_mangle)]
pub extern "C" fn oida_netz_antwort_koepfe() -> *mut ListenKopf {
    ANTWORT.with(|a| {
        let flach: Vec<String> = a.borrow().koepfe.iter().flat_map(|(name, wert)| [name.clone(), wert.clone()]).collect();
        liste_aus_texten(&flach)
    })
}

/// Schreibt den Inhalt der letzten Antwort in eine Datei, Byte für Byte.
///
/// # Safety
/// `pfad` muss ein gültiger Text sein.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn oida_netz_antwort_speichern(pfad: *const TextKopf) -> bool {
    let pfad = unsafe { text_von(pfad) };
    match ANTWORT.with(|a| std::fs::write(&pfad, &a.borrow().inhalt)) {
        Ok(()) => true,
        Err(fehler) => {
            merke_fehler(&fehler);
            false
        }
    }
}

/// Zerlegt die Köpfe einer Antwort ("Nom: Wert", eine Zeile je Kopf).
fn antwort_koepfe<'a>(zeilen: impl Iterator<Item = &'a str>) -> Vec<(String, String)> {
    zeilen
        .filter_map(|zeile| zeile.split_once(':'))
        .map(|(name, wert)| (name.trim().to_ascii_lowercase(), wert.trim().to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

// ------------------------------------------------- HTTP unter Windows: WinHTTP

#[cfg(windows)]
mod windows {
    use super::{Adresse, Antwort, antwort_koepfe, meldung, wartezeit};
    use std::ffi::c_void;
    use std::sync::OnceLock;

    type Griff = *mut c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> Griff;
        fn GetProcAddress(bibliothek: Griff, name: *const u8) -> Griff;
        fn GetLastError() -> u32;
    }

    /// Die Funktionen aus winhttp.dll (die Namen und Angaben stehen in winhttp.h).
    struct Winhttp {
        open: unsafe extern "system" fn(*const u16, u32, *const u16, *const u16, u32) -> Griff,
        set_timeouts: unsafe extern "system" fn(Griff, i32, i32, i32, i32) -> i32,
        connect: unsafe extern "system" fn(Griff, *const u16, u16, u32) -> Griff,
        open_request: unsafe extern "system" fn(Griff, *const u16, *const u16, *const u16, *const u16, *const *const u16, u32) -> Griff,
        add_request_headers: unsafe extern "system" fn(Griff, *const u16, u32, u32) -> i32,
        send_request: unsafe extern "system" fn(Griff, *const u16, u32, *const c_void, u32, u32, usize) -> i32,
        receive_response: unsafe extern "system" fn(Griff, *mut c_void) -> i32,
        query_headers: unsafe extern "system" fn(Griff, u32, *const u16, *mut c_void, *mut u32, *mut u32) -> i32,
        query_data_available: unsafe extern "system" fn(Griff, *mut u32) -> i32,
        read_data: unsafe extern "system" fn(Griff, *mut c_void, u32, *mut u32) -> i32,
        close_handle: unsafe extern "system" fn(Griff) -> i32,
    }

    // Die Zeiger zeigen auf Code in winhttp.dll, der bis zum Ende des Programms geladen bleibt.
    unsafe impl Sync for Winhttp {}
    unsafe impl Send for Winhttp {}

    fn breit(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn winhttp() -> Result<&'static Winhttp, String> {
        static GELADEN: OnceLock<Option<Winhttp>> = OnceLock::new();
        GELADEN
            .get_or_init(|| unsafe {
                let dll = LoadLibraryW(breit("winhttp.dll").as_ptr());
                if dll.is_null() {
                    return None;
                }
                let namen = [
                    "WinHttpOpen",
                    "WinHttpSetTimeouts",
                    "WinHttpConnect",
                    "WinHttpOpenRequest",
                    "WinHttpAddRequestHeaders",
                    "WinHttpSendRequest",
                    "WinHttpReceiveResponse",
                    "WinHttpQueryHeaders",
                    "WinHttpQueryDataAvailable",
                    "WinHttpReadData",
                    "WinHttpCloseHandle",
                ];
                let z: Vec<Griff> = namen.iter().map(|name| GetProcAddress(dll, format!("{name}\0").as_ptr())).collect();
                // Erst wenn alle da sind, werden aus den Zeigern Funktionen.
                if z.iter().any(|zeiger| zeiger.is_null()) {
                    return None;
                }
                Some(Winhttp {
                    open: std::mem::transmute(z[0]),
                    set_timeouts: std::mem::transmute(z[1]),
                    connect: std::mem::transmute(z[2]),
                    open_request: std::mem::transmute(z[3]),
                    add_request_headers: std::mem::transmute(z[4]),
                    send_request: std::mem::transmute(z[5]),
                    receive_response: std::mem::transmute(z[6]),
                    query_headers: std::mem::transmute(z[7]),
                    query_data_available: std::mem::transmute(z[8]),
                    read_data: std::mem::transmute(z[9]),
                    close_handle: std::mem::transmute(z[10]),
                })
            })
            .as_ref()
            .ok_or_else(|| "Windows gibt ma des Netz ned her (winhttp.dll fehlt).".to_string())
    }

    /// Ein Griff von WinHTTP, der beim Verlassen zugemacht wird.
    struct Offen(&'static Winhttp, Griff);

    impl Drop for Offen {
        fn drop(&mut self) {
            unsafe { (self.0.close_handle)(self.1) };
        }
    }

    const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
    const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
    const WINHTTP_FLAG_ESCAPE_DISABLE: u32 = 0x40;
    const WINHTTP_FLAG_ESCAPE_DISABLE_QUERY: u32 = 0x80;
    const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
    const WINHTTP_ADDREQ_FLAG_ADD: u32 = 0x2000_0000;
    const WINHTTP_ADDREQ_FLAG_REPLACE: u32 = 0x8000_0000;
    const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
    const WINHTTP_QUERY_RAW_HEADERS_CRLF: u32 = 22;
    const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;

    /// Was der letzte Fehler von WinHTTP bedeutet (die Nummern stehen in winhttp.h ab 12000).
    fn fehler(adresse: &Adresse) -> String {
        let host = adresse.host.as_str();
        match unsafe { GetLastError() } {
            12002 => meldung::stumm(host),
            12007 => meldung::unbekannt(host),
            12029 => format!("{host} nimmt auf Port {} kane Vabindungen au.", adresse.port),
            12030 | 12152 => meldung::abgerissen(host),
            12175 | 12037 | 12038 | 12045 | 12057 | 12169 | 12170 => meldung::zertifikat(host, ""),
            12156 => "De Seitn schickt mi im Kreis oda vo https auf http um; des moch i ned.".to_string(),
            nummer => format!("Bei {host} is wos schiefgaunga (Windows sogt: Fehler {nummer})."),
        }
    }

    pub fn anfrage(methode: &str, adresse: &Adresse, koepfe: &[(String, String)], inhalt: &[u8]) -> Result<Antwort, String> {
        let w = winhttp()?;
        let host = adresse.host.as_str();
        let pruef = |griff: Griff| if griff.is_null() { Err(fehler(adresse)) } else { Ok(Offen(w, griff)) };
        let klappt = |ergebnis: i32| if ergebnis == 0 { Err(fehler(adresse)) } else { Ok(()) };
        unsafe {
            let wer = breit(concat!("OIDA/", env!("CARGO_PKG_VERSION")));
            // Der Proxy, den Windows eingestellt hat; ältere Windows kennen die Automatik nicht.
            let mut sitzung = (w.open)(wer.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, std::ptr::null(), std::ptr::null(), 0);
            if sitzung.is_null() {
                sitzung = (w.open)(wer.as_ptr(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, std::ptr::null(), std::ptr::null(), 0);
            }
            let sitzung = pruef(sitzung)?;
            let ms = wartezeit().map_or(0, |z| z.as_millis().min(i32::MAX as u128) as i32);
            klappt((w.set_timeouts)(sitzung.1, ms, ms, ms, ms))?;
            let verbindung = pruef((w.connect)(sitzung.1, breit(host).as_ptr(), adresse.port, 0))?;
            // Der Pfad ist schon verpackt; WinHTTP soll ihn nicht noch einmal anfassen.
            let schalter = WINHTTP_FLAG_ESCAPE_DISABLE
                | WINHTTP_FLAG_ESCAPE_DISABLE_QUERY
                | if adresse.sicher { WINHTTP_FLAG_SECURE } else { 0 };
            let auftrag = pruef((w.open_request)(
                verbindung.1,
                breit(methode).as_ptr(),
                breit(&adresse.pfad).as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                schalter,
            ))?;
            if !koepfe.is_empty() {
                let alle: String = koepfe.iter().map(|(name, wert)| format!("{name}: {wert}\r\n")).collect();
                klappt((w.add_request_headers)(
                    auftrag.1,
                    breit(&alle).as_ptr(),
                    u32::MAX,
                    WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
                ))?;
            }
            let laenge = u32::try_from(inhalt.len()).map_err(|_| "Der Inhalt is z'groß zum Schickn (üba 4 GB).".to_string())?;
            // Ohne Inhalt will WinHTTP hier ausdrücklich nichts (null) sehen.
            let daten = if inhalt.is_empty() { std::ptr::null() } else { inhalt.as_ptr().cast() };
            klappt((w.send_request)(auftrag.1, std::ptr::null(), 0, daten, laenge, laenge, 0))?;
            klappt((w.receive_response)(auftrag.1, std::ptr::null_mut()))?;

            let (mut code, mut groesse) = (0u32, 4u32);
            klappt((w.query_headers)(
                auftrag.1,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                std::ptr::null(),
                (&raw mut code).cast(),
                &mut groesse,
                std::ptr::null_mut(),
            ))?;
            // Alle Köpfe als ein Text; der erste Aufruf sagt nur, wie viel Platz es braucht.
            let mut bytes = 0u32;
            (w.query_headers)(auftrag.1, WINHTTP_QUERY_RAW_HEADERS_CRLF, std::ptr::null(), std::ptr::null_mut(), &mut bytes, std::ptr::null_mut());
            let mut roh = vec![0u16; bytes as usize / 2 + 1];
            let mut platz = (roh.len() * 2) as u32;
            klappt((w.query_headers)(
                auftrag.1,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                std::ptr::null(),
                roh.as_mut_ptr().cast(),
                &mut platz,
                std::ptr::null_mut(),
            ))?;
            let roh = String::from_utf16_lossy(&roh[..platz as usize / 2]);
            // Die erste Zeile ist "HTTP/1.1 200 OK", danach kommen die Köpfe.
            let koepfe = antwort_koepfe(roh.lines().skip(1));

            let mut inhalt = Vec::new();
            loop {
                let mut da = 0u32;
                klappt((w.query_data_available)(auftrag.1, &mut da))?;
                if da == 0 {
                    break;
                }
                let anfang = inhalt.len();
                inhalt.resize(anfang + da as usize, 0);
                let mut gelesen = 0u32;
                klappt((w.read_data)(auftrag.1, inhalt[anfang..].as_mut_ptr().cast(), da, &mut gelesen))?;
                inhalt.truncate(anfang + gelesen as usize);
                if gelesen == 0 {
                    break;
                }
            }
            Ok(Antwort { code: i64::from(code), koepfe, inhalt })
        }
    }
}

// ------------------------------------- HTTP auf den anderen Systemen: selbst

#[cfg(not(windows))]
mod eigen {
    use super::{Adresse, Antwort, antwort_koepfe, meldung, verbinde};
    use std::io::{BufRead, BufReader, Read, Write};

    /// Worüber die Anfrage geht: eine gewöhnliche Verbindung oder eine verschlüsselte.
    trait Strom: Read + Write {}
    impl<T: Read + Write> Strom for T {}

    fn ueblich(adresse: &Adresse) -> bool {
        adresse.port == if adresse.sicher { 443 } else { 80 }
    }

    pub fn anfrage(methode: &str, mut adresse: Adresse, koepfe: &[(String, String)], inhalt: &[u8]) -> Result<Antwort, String> {
        let mut methode = methode.to_string();
        let mut inhalt = inhalt.to_vec();
        let mut koepfe = koepfe.to_vec();
        for _ in 0..=10 {
            let antwort = einmal(&methode, &adresse, &koepfe, &inhalt)?;
            let ziel = antwort.koepfe.iter().find(|(name, _)| name == "location").map(|(_, wert)| wert.clone());
            let Some(ziel) = ziel.filter(|_| matches!(antwort.code, 301 | 302 | 303 | 307 | 308)) else {
                return Ok(antwort);
            };
            // Die Seite ist umgezogen: dorthin gehen, wo sie jetzt ist.
            let neu = if ziel.contains("://") {
                Adresse::zerlegen(&ziel)?
            } else {
                let pfad = if ziel.starts_with('/') {
                    ziel
                } else {
                    // Ein Ziel ohne / am Anfang gilt vom Ordner der jetzigen Seite aus.
                    let ohne_frage = adresse.pfad.split('?').next().unwrap_or("/");
                    format!("{}/{ziel}", &ohne_frage[..ohne_frage.rfind('/').unwrap_or(0)])
                };
                Adresse::zerlegen(&format!(
                    "{}://{}:{}{pfad}",
                    if adresse.sicher { "https" } else { "http" },
                    if adresse.host.contains(':') { format!("[{}]", adresse.host) } else { adresse.host.clone() },
                    adresse.port
                ))?
            };
            if adresse.sicher && !neu.sicher {
                return Err("De Seitn schickt mi im Kreis oda vo https auf http um; des moch i ned.".to_string());
            }
            // Was nur für den ersten Rechner gedacht war, bekommt ein anderer nicht zu sehen.
            if neu.host != adresse.host {
                koepfe.retain(|(name, _)| !name.eq_ignore_ascii_case("authorization") && !name.eq_ignore_ascii_case("cookie"));
            }
            // Nach "schau woanders nach" wird geholt statt noch einmal geschickt.
            if antwort.code == 303 || (matches!(antwort.code, 301 | 302) && methode == "POST") {
                methode = "GET".to_string();
                inhalt.clear();
                koepfe.retain(|(name, _)| !name.to_ascii_lowercase().starts_with("content-"));
            }
            adresse = neu;
        }
        Err("De Seitn schickt mi im Kreis oda vo https auf http um; des moch i ned.".to_string())
    }

    /// Eine Anfrage an einen Rechner, ohne Umleitungen zu folgen.
    fn einmal(methode: &str, adresse: &Adresse, koepfe: &[(String, String)], inhalt: &[u8]) -> Result<Antwort, String> {
        let host = adresse.host.as_str();
        let roh = verbinde(host, adresse.port)?;
        let mut strom: Box<dyn Strom> =
            if adresse.sicher { Box::new(tls::Sicher::neu(roh, host)?) } else { Box::new(roh) };

        let hat = |gesucht: &str| koepfe.iter().any(|(name, _)| name.eq_ignore_ascii_case(gesucht));
        let wer = if host.contains(':') { format!("[{host}]") } else { host.to_string() };
        let mut kopf = format!("{methode} {} HTTP/1.1\r\n", adresse.pfad);
        if !hat("host") {
            kopf.push_str(&if ueblich(adresse) { format!("Host: {wer}\r\n") } else { format!("Host: {wer}:{}\r\n", adresse.port) });
        }
        if !hat("user-agent") {
            kopf.push_str(concat!("User-Agent: OIDA/", env!("CARGO_PKG_VERSION"), "\r\n"));
        }
        if !hat("accept") {
            kopf.push_str("Accept: */*\r\n");
        }
        for (name, wert) in koepfe {
            // Diese drei bestimmt OIDA selbst, damit Anfrage und Antwort zusammenpassen.
            if !["connection", "content-length", "accept-encoding", "transfer-encoding"].contains(&name.to_ascii_lowercase().as_str()) {
                kopf.push_str(&format!("{name}: {wert}\r\n"));
            }
        }
        // Ungepackt, und danach ist Schluss: So ist klar, wo die Antwort aufhört.
        kopf.push_str("Accept-Encoding: identity\r\nConnection: close\r\n");
        if !inhalt.is_empty() || matches!(methode, "POST" | "PUT" | "PATCH") {
            kopf.push_str(&format!("Content-Length: {}\r\n", inhalt.len()));
        }
        kopf.push_str("\r\n");
        let geschickt = strom.write_all(kopf.as_bytes()).and_then(|()| strom.write_all(inhalt)).and_then(|()| strom.flush());
        geschickt.map_err(|f| lesefehler(host, &f))?;

        let mut leser = BufReader::new(strom);
        loop {
            let (code, koepfe) = antwort_kopf(&mut leser, host)?;
            // 100 "moch weida" und ähnliche sind nur Zwischenrufe; die echte Antwort kommt danach.
            if (100..200).contains(&code) {
                continue;
            }
            let wert = |gesucht: &str| koepfe.iter().find(|(name, _)| name == gesucht).map(|(_, wert)| wert.as_str());
            let mut inhalt = Vec::new();
            if methode == "HEAD" || code == 204 || code == 304 {
                // Diese Antworten haben keinen Inhalt, auch wenn eine Länge dabeisteht.
            } else if wert("transfer-encoding").is_some_and(|w| w.to_ascii_lowercase().contains("chunked")) {
                stueckweise(&mut leser, &mut inhalt).map_err(|f| lesefehler(host, &f))?;
            } else if let Some(laenge) = wert("content-length").and_then(|w| w.parse::<usize>().ok()) {
                inhalt.resize(laenge, 0);
                leser.read_exact(&mut inhalt).map_err(|f| lesefehler(host, &f))?;
            } else {
                leser.read_to_end(&mut inhalt).map_err(|f| lesefehler(host, &f))?;
            }
            return Ok(Antwort { code, koepfe, inhalt });
        }
    }

    fn lesefehler(host: &str, fehler: &std::io::Error) -> String {
        use std::io::ErrorKind::*;
        match fehler.kind() {
            TimedOut | WouldBlock => meldung::stumm(host),
            _ => meldung::abgerissen(host),
        }
    }

    /// Eine Zeile ohne ihr Ende; länger als 64 KB darf sie nicht sein.
    fn zeile(leser: &mut impl BufRead) -> std::io::Result<String> {
        let mut roh = Vec::new();
        leser.by_ref().take(65_536).read_until(b'\n', &mut roh)?;
        if roh.last() != Some(&b'\n') {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        Ok(String::from_utf8_lossy(&roh).trim_end_matches(['\r', '\n']).to_string())
    }

    /// Die erste Zeile der Antwort und ihre Köpfe.
    fn antwort_kopf(leser: &mut impl BufRead, host: &str) -> Result<(i64, Vec<(String, String)>), String> {
        let kaputt = || format!("{host} redt ka HTTP: Mit dera Antwort kaun i nix aufaunga.");
        let erste = zeile(leser).map_err(|f| lesefehler(host, &f))?;
        // "HTTP/1.1 200 OK"
        let code = erste
            .strip_prefix("HTTP/")
            .and_then(|rest| rest.split_whitespace().nth(1))
            .and_then(|code| code.parse::<i64>().ok())
            .filter(|code| (100..1000).contains(code))
            .ok_or_else(kaputt)?;
        let mut zeilen = Vec::new();
        loop {
            let naechste = zeile(leser).map_err(|f| lesefehler(host, &f))?;
            if naechste.is_empty() {
                break;
            }
            if zeilen.len() >= 500 {
                return Err(kaputt());
            }
            zeilen.push(naechste);
        }
        Ok((code, antwort_koepfe(zeilen.iter().map(String::as_str))))
    }

    /// Ein Inhalt, der in Stücken kommt: vor jedem Stück steht seine Länge (mit sechzehn Ziffern).
    fn stueckweise(leser: &mut impl BufRead, inhalt: &mut Vec<u8>) -> std::io::Result<()> {
        let kaputt = || std::io::Error::from(std::io::ErrorKind::InvalidData);
        loop {
            let kopf = zeile(leser)?;
            let laenge = usize::from_str_radix(kopf.split(';').next().unwrap_or("").trim(), 16).map_err(|_| kaputt())?;
            if laenge == 0 {
                // Danach können noch Köpfe kommen; die braucht hier niemand.
                while !zeile(leser)?.is_empty() {}
                return Ok(());
            }
            let anfang = inhalt.len();
            inhalt.resize(anfang + laenge, 0);
            leser.read_exact(&mut inhalt[anfang..])?;
            if !zeile(leser)?.is_empty() {
                return Err(kaputt());
            }
        }
    }

    /// Verschlüsselte Verbindungen über die OpenSSL-Bibliothek des Systems. Sie wird erst
    /// beim ersten HTTPS geladen (`dlopen`): Ein Programm ohne HTTPS braucht sie nicht, und
    /// zum Übersetzen müssen ihre Entwicklerdateien nicht installiert sein.
    mod tls {
        use super::super::meldung;
        use std::ffi::{CStr, CString, c_char, c_int, c_long, c_ulong, c_void};
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::os::fd::AsRawFd;
        use std::sync::OnceLock;

        unsafe extern "C" {
            fn dlopen(name: *const c_char, art: c_int) -> *mut c_void;
            fn dlsym(bibliothek: *mut c_void, name: *const c_char) -> *mut c_void;
        }
        const RTLD_NOW: c_int = 2;

        /// Die Funktionen aus libssl und libcrypto (die Angaben stehen in openssl/ssl.h).
        struct Openssl {
            tls_client_method: unsafe extern "C" fn() -> *const c_void,
            ctx_new: unsafe extern "C" fn(*const c_void) -> *mut c_void,
            ctx_free: unsafe extern "C" fn(*mut c_void),
            ctx_set_default_verify_paths: unsafe extern "C" fn(*mut c_void) -> c_int,
            ctx_set_verify: unsafe extern "C" fn(*mut c_void, c_int, *const c_void),
            ctx_ctrl: unsafe extern "C" fn(*mut c_void, c_int, c_long, *mut c_void) -> c_long,
            new: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
            free: unsafe extern "C" fn(*mut c_void),
            set_fd: unsafe extern "C" fn(*mut c_void, c_int) -> c_int,
            ctrl: unsafe extern "C" fn(*mut c_void, c_int, c_long, *mut c_void) -> c_long,
            set1_host: unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int,
            connect: unsafe extern "C" fn(*mut c_void) -> c_int,
            get_verify_result: unsafe extern "C" fn(*const c_void) -> c_long,
            read: unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> c_int,
            write: unsafe extern "C" fn(*mut c_void, *const c_void, c_int) -> c_int,
            get_error: unsafe extern "C" fn(*const c_void, c_int) -> c_int,
            shutdown: unsafe extern "C" fn(*mut c_void) -> c_int,
            verify_cert_error_string: unsafe extern "C" fn(c_long) -> *const c_char,
            err_clear_error: unsafe extern "C" fn(),
            err_get_error: unsafe extern "C" fn() -> c_ulong,
        }

        unsafe impl Sync for Openssl {}
        unsafe impl Send for Openssl {}

        const SSL_VERIFY_PEER: c_int = 1;
        const SSL_CTRL_SET_TLSEXT_HOSTNAME: c_int = 55;
        const SSL_CTRL_SET_MIN_PROTO_VERSION: c_int = 123;
        const TLS1_2_VERSION: c_long = 0x0303;
        const SSL_ERROR_WANT_READ: c_int = 2;
        const SSL_ERROR_WANT_WRITE: c_int = 3;
        const SSL_ERROR_SYSCALL: c_int = 5;
        const SSL_ERROR_ZERO_RETURN: c_int = 6;

        fn openssl() -> Result<&'static Openssl, String> {
            static GELADEN: OnceLock<Option<Openssl>> = OnceLock::new();
            GELADEN
                .get_or_init(|| unsafe {
                    let lade = |namen: &[&str]| {
                        namen.iter().map(|name| dlopen(CString::new(*name).unwrap().as_ptr(), RTLD_NOW)).find(|b| !b.is_null())
                    };
                    let ssl = lade(&["libssl.so.3", "libssl.so.1.1", "libssl.so"])?;
                    let crypto = lade(&["libcrypto.so.3", "libcrypto.so.1.1", "libcrypto.so"])?;
                    let namen = [
                        (ssl, "TLS_client_method"),
                        (ssl, "SSL_CTX_new"),
                        (ssl, "SSL_CTX_free"),
                        (ssl, "SSL_CTX_set_default_verify_paths"),
                        (ssl, "SSL_CTX_set_verify"),
                        (ssl, "SSL_CTX_ctrl"),
                        (ssl, "SSL_new"),
                        (ssl, "SSL_free"),
                        (ssl, "SSL_set_fd"),
                        (ssl, "SSL_ctrl"),
                        (ssl, "SSL_set1_host"),
                        (ssl, "SSL_connect"),
                        (ssl, "SSL_get_verify_result"),
                        (ssl, "SSL_read"),
                        (ssl, "SSL_write"),
                        (ssl, "SSL_get_error"),
                        (ssl, "SSL_shutdown"),
                        (crypto, "X509_verify_cert_error_string"),
                        (crypto, "ERR_clear_error"),
                        (crypto, "ERR_get_error"),
                    ];
                    let z: Vec<*mut c_void> =
                        namen.iter().map(|(bibliothek, name)| dlsym(*bibliothek, CString::new(*name).unwrap().as_ptr())).collect();
                    // Erst wenn alle da sind, werden aus den Zeigern Funktionen.
                    if z.iter().any(|zeiger| zeiger.is_null()) {
                        return None;
                    }
                    Some(Openssl {
                        tls_client_method: std::mem::transmute(z[0]),
                        ctx_new: std::mem::transmute(z[1]),
                        ctx_free: std::mem::transmute(z[2]),
                        ctx_set_default_verify_paths: std::mem::transmute(z[3]),
                        ctx_set_verify: std::mem::transmute(z[4]),
                        ctx_ctrl: std::mem::transmute(z[5]),
                        new: std::mem::transmute(z[6]),
                        free: std::mem::transmute(z[7]),
                        set_fd: std::mem::transmute(z[8]),
                        ctrl: std::mem::transmute(z[9]),
                        set1_host: std::mem::transmute(z[10]),
                        connect: std::mem::transmute(z[11]),
                        get_verify_result: std::mem::transmute(z[12]),
                        read: std::mem::transmute(z[13]),
                        write: std::mem::transmute(z[14]),
                        get_error: std::mem::transmute(z[15]),
                        shutdown: std::mem::transmute(z[16]),
                        verify_cert_error_string: std::mem::transmute(z[17]),
                        err_clear_error: std::mem::transmute(z[18]),
                        err_get_error: std::mem::transmute(z[19]),
                    })
                })
                .as_ref()
                .ok_or_else(|| {
                    "Fia https brauch i OpenSSL (libssl), des find i auf dem Rechner ned. \
                     Installier's mit da Paketverwoitung (libssl3 oda openssl)."
                        .to_string()
                })
        }

        /// Eine verschlüsselte Verbindung. Das Zertifikat der anderen Seite ist geprüft:
        /// von einer Stelle ausgestellt, der das System traut, und für genau diesen Rechner.
        pub struct Sicher {
            o: &'static Openssl,
            ctx: *mut c_void,
            ssl: *mut c_void,
            // Die Verbindung darunter muss so lange leben wie die Verschlüsselung darüber.
            _strom: TcpStream,
        }

        impl Sicher {
            pub fn neu(strom: TcpStream, host: &str) -> Result<Sicher, String> {
                let o = openssl()?;
                let kaputt = || format!("De sichare Vabindung mit {host} kummt ned zstaund.");
                let name = CString::new(host).map_err(|_| kaputt())?;
                unsafe {
                    let ctx = (o.ctx_new)((o.tls_client_method)());
                    if ctx.is_null() {
                        return Err(kaputt());
                    }
                    (o.ctx_set_default_verify_paths)(ctx);
                    (o.ctx_set_verify)(ctx, SSL_VERIFY_PEER, std::ptr::null());
                    // Älteres als TLS 1.2 gilt nicht mehr als sicher.
                    (o.ctx_ctrl)(ctx, SSL_CTRL_SET_MIN_PROTO_VERSION, TLS1_2_VERSION, std::ptr::null_mut());
                    let ssl = (o.new)(ctx);
                    if ssl.is_null() {
                        (o.ctx_free)(ctx);
                        return Err(kaputt());
                    }
                    let sicher = Sicher { o, ctx, ssl, _strom: strom };
                    // Der anderen Seite sagen, welcher Rechner gemeint ist (bei einer Adresse aus Zahlen nicht).
                    if host.parse::<std::net::IpAddr>().is_err() {
                        (o.ctrl)(ssl, SSL_CTRL_SET_TLSEXT_HOSTNAME, 0, name.as_ptr() as *mut c_void);
                    }
                    // Das Zertifikat muss für genau diesen Namen gelten.
                    if (o.set1_host)(ssl, name.as_ptr()) != 1 || (o.set_fd)(ssl, sicher._strom.as_raw_fd()) != 1 {
                        return Err(kaputt());
                    }
                    (o.err_clear_error)();
                    let ergebnis = (o.connect)(ssl);
                    if ergebnis != 1 {
                        let pruefung = (o.get_verify_result)(ssl);
                        let art = (o.get_error)(ssl, ergebnis);
                        while (o.err_get_error)() != 0 {}
                        if pruefung != 0 {
                            let grund = CStr::from_ptr((o.verify_cert_error_string)(pruefung)).to_string_lossy().into_owned();
                            return Err(meldung::zertifikat(host, &grund));
                        }
                        // Die andere Seite hat mitten im Aushandeln aufgelegt: Das liegt nicht am Zertifikat.
                        if art == SSL_ERROR_SYSCALL {
                            return Err(meldung::abgerissen(host));
                        }
                        return Err(kaputt());
                    }
                    Ok(sicher)
                }
            }

            fn fehler(&self, ergebnis: c_int) -> std::io::Result<usize> {
                use std::io::ErrorKind::*;
                let art = unsafe { (self.o.get_error)(self.ssl, ergebnis) };
                unsafe { while (self.o.err_get_error)() != 0 {} }
                match art {
                    // Die andere Seite hat zugemacht (ordentlich oder einfach so): Das ist das Ende.
                    SSL_ERROR_ZERO_RETURN | SSL_ERROR_SYSCALL => Ok(0),
                    SSL_ERROR_WANT_READ | SSL_ERROR_WANT_WRITE => Err(TimedOut.into()),
                    _ => Err(ConnectionAborted.into()),
                }
            }
        }

        impl Read for Sicher {
            fn read(&mut self, puffer: &mut [u8]) -> std::io::Result<usize> {
                let platz = puffer.len().min(c_int::MAX as usize) as c_int;
                match unsafe { (self.o.read)(self.ssl, puffer.as_mut_ptr().cast(), platz) } {
                    anzahl if anzahl > 0 => Ok(anzahl as usize),
                    ergebnis => self.fehler(ergebnis),
                }
            }
        }

        impl Write for Sicher {
            fn write(&mut self, daten: &[u8]) -> std::io::Result<usize> {
                if daten.is_empty() {
                    return Ok(0);
                }
                let laenge = daten.len().min(c_int::MAX as usize) as c_int;
                match unsafe { (self.o.write)(self.ssl, daten.as_ptr().cast(), laenge) } {
                    anzahl if anzahl > 0 => Ok(anzahl as usize),
                    ergebnis => match self.fehler(ergebnis) {
                        Ok(_) => Err(std::io::ErrorKind::ConnectionAborted.into()),
                        Err(fehler) => Err(fehler),
                    },
                }
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl Drop for Sicher {
            fn drop(&mut self) {
                unsafe {
                    (self.o.shutdown)(self.ssl);
                    (self.o.free)(self.ssl);
                    (self.o.ctx_free)(self.ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(inhalt: &str) -> *mut TextKopf {
        neuer_text(inhalt.as_bytes())
    }

    fn inhalt(text: *mut TextKopf) -> String {
        assert!(!text.is_null(), "da sollte ein Text sein");
        String::from_utf8(unsafe { bytes(text) }.to_vec()).unwrap()
    }

    fn letzter_fehler() -> String {
        inhalt(super::super::dateien::oida_datei_fehler())
    }

    /// Die Tests mit echten Verbindungen laufen einer nach dem anderen: Sonst schnappt sich
    /// einer den Port, den ein anderer gerade freigegeben hat und für unbesetzt hält.
    static NACHEINANDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn allein() -> std::sync::MutexGuard<'static, ()> {
        NACHEINANDER.lock().unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    fn zahlen(liste: *mut ListenKopf) -> Vec<i64> {
        unsafe { std::slice::from_raw_parts((*liste).daten as *const i64, (*liste).laenge as usize).to_vec() }
    }

    #[test]
    fn adressen_zerlegen() {
        let a = |sicher, host: &str, port, pfad: &str| Ok(Adresse { sicher, host: host.to_string(), port, pfad: pfad.to_string() });
        assert_eq!(Adresse::zerlegen("http://example.com"), a(false, "example.com", 80, "/"));
        assert_eq!(Adresse::zerlegen(" HTTPS://Example.COM/a/b?x=1&y=2#unten "), a(true, "example.com", 443, "/a/b?x=1&y=2"));
        assert_eq!(Adresse::zerlegen("http://localhost:8080?frog"), a(false, "localhost", 8080, "/?frog"));
        assert_eq!(Adresse::zerlegen("https://[::1]:8443/x"), a(true, "::1", 8443, "/x"));
        assert_eq!(Adresse::zerlegen("http://[::1]"), a(false, "::1", 80, "/"));
        // Leerzeichen und Umlaute im Pfad werden verpackt; was schon verpackt ist, bleibt.
        assert_eq!(Adresse::zerlegen("http://a/grüß di%20oida"), a(false, "a", 80, "/gr%C3%BC%C3%9F%20di%20oida"));
        for kaputt in ["example.com", "ftp://a/b", "http://", "http:///pfad", "http://a:0/", "http://a:x/", "http://a:70000/", "http://nam:pw@a/", "http://grüß.at/", "http://a b/"] {
            assert!(Adresse::zerlegen(kaputt).is_err(), "{kaputt}");
        }
        assert_eq!(Adresse::zerlegen("servas").unwrap_err(), "De Adress 'servas' muass mit http:// oda https:// aufaunga.");
    }

    #[test]
    fn koepfe_einer_anfrage() {
        let zeilen = |z: &[&str]| z.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        assert_eq!(
            koepfe_lesen(&zeilen(&["Accept: text/plain", "X-Mei_Kopf:  a: b "])),
            Ok(vec![("Accept".to_string(), "text/plain".to_string()), ("X-Mei_Kopf".to_string(), "a: b".to_string())])
        );
        // Kein Name, kein Doppelpunkt, ein Leerzeichen im Namen, ein Zeilenende im Wert: alles kein Kopf.
        for kaputt in ["ohne Doppelpunkt", ": wert", "zwa wörter: x", "A: b\r\nB: c"] {
            assert!(koepfe_lesen(&zeilen(&[kaputt])).is_err(), "{kaputt}");
        }
    }

    #[test]
    fn verbindung_auf_demselben_rechner() {
        let _allein = allein();
        // Ein Programm kann mit sich selbst reden: horchen, verbinden, annehmen.
        let horcher = oida_netz_horchen(0, false);
        assert!(horcher >= 0);
        let port = oida_netz_horcher_port(horcher);
        assert!(port > 0);
        let hin = unsafe { oida_netz_verbinden(text("127.0.0.1"), port) };
        let her = oida_netz_annehmen(horcher);
        assert!(hin >= 0 && her >= 0 && hin != her, "{}", letzter_fehler());
        assert!(inhalt(oida_netz_gegenueber(hin)).ends_with(&format!(":{port}")));
        unsafe {
            assert!(oida_netz_schicken(hin, text("Servas!\r\nzweite Zeile\nohne Ende")));
            assert_eq!(inhalt(oida_netz_zeile(her)), "Servas!");
            assert_eq!(inhalt(oida_netz_zeile(her)), "zweite Zeile");
            // Zurück gehen Bytes.
            assert!(oida_netz_bytes_schicken(her, liste_aus_zahlen(&[79, 73, 68, 65, 0, 255])));
            assert_eq!(zahlen(oida_netz_bytes_lesen(hin, 100)), vec![79, 73, 68, 65, 0, 255]);
            assert!(!oida_netz_bytes_schicken(her, liste_aus_zahlen(&[1, 256])));
            assert_eq!(letzter_fehler(), "An da Stö 1 steht 256, a Byte geht oba nur vo 0 bis 255.");
        }
        // Nach dem Zumachen kommt drüben noch der Rest und dann das Ende.
        oida_netz_zu(hin);
        assert_eq!(inhalt(oida_netz_zeile(her)), "ohne Ende");
        assert!(oida_netz_zeile(her).is_null());
        assert!(zahlen(oida_netz_bytes_lesen(her, 10)).is_empty());
        // Eine zugemachte Verbindung gibt es nicht mehr; zweimal zumachen schadet nicht.
        unsafe { assert!(!oida_netz_schicken(hin, text("x"))) };
        assert_eq!(letzter_fehler(), "De Vabindung is scho zua.");
        oida_netz_zu(hin);
        oida_netz_zu(her);
        oida_netz_zu(4711);
        // Auf demselben Port kann nur einer horchen.
        assert_eq!(oida_netz_horchen(port, false), -1);
        assert_eq!(letzter_fehler(), "Der Port is scho bsetzt.");
        oida_netz_horcher_zu(horcher);
        assert_eq!(oida_netz_horcher_port(horcher), -1);
        assert_eq!(oida_netz_annehmen(horcher), -1);
        // Jetzt nimmt dort niemand mehr an.
        assert_eq!(unsafe { oida_netz_verbinden(text("127.0.0.1"), port) }, -1);
        assert_eq!(letzter_fehler(), format!("127.0.0.1 nimmt auf Port {port} kane Vabindungen au."));
        assert_eq!(unsafe { oida_netz_verbinden(text("127.0.0.1"), 70000) }, -1);
    }

    /// Ein kleiner Server für die Tests: Jede Anfrage (Kopf und Inhalt als Text) bekommt die
    /// Antwort, die `antwort` dazu liefert. Gibt den Port zurück.
    fn probe_server(antwort: impl Fn(&str) -> Vec<u8> + Send + 'static) -> i64 {
        let horcher = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = horcher.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut strom in horcher.incoming().flatten() {
                let mut leser = BufReader::new(strom.try_clone().unwrap());
                let mut kopf = String::new();
                loop {
                    let mut zeile = String::new();
                    if leser.read_line(&mut zeile).unwrap_or(0) == 0 || zeile == "\r\n" {
                        break;
                    }
                    kopf.push_str(&zeile);
                }
                let laenge = kopf
                    .lines()
                    .find_map(|z| z.to_ascii_lowercase().strip_prefix("content-length:").map(|w| w.trim().parse::<usize>().unwrap()))
                    .unwrap_or(0);
                let mut inhalt = vec![0; laenge];
                let _ = leser.read_exact(&mut inhalt);
                let _ = strom.write_all(&antwort(&format!("{kopf}\r\n{}", String::from_utf8_lossy(&inhalt))));
                let _ = strom.shutdown(Shutdown::Both);
            }
        });
        i64::from(port)
    }

    fn einfach(code: &str, koepfe: &str, inhalt: &str) -> Vec<u8> {
        format!("HTTP/1.1 {code}\r\nContent-Length: {}\r\n{koepfe}\r\n{inhalt}", inhalt.len()).into_bytes()
    }

    /// Schickt eine Anfrage und gibt Code und Inhalt zurück, oder die Meldung.
    fn frag(methode: &str, adresse: &str, koepfe: &[&str], inhalt_text: &str) -> Result<(i64, String), String> {
        let koepfe: Vec<String> = koepfe.iter().map(|k| k.to_string()).collect();
        let geklappt = unsafe { oida_netz_anfrage(text(methode), text(adresse), liste_aus_texten(&koepfe), text(inhalt_text)) };
        if geklappt { Ok((oida_netz_antwort_code(), inhalt(oida_netz_antwort_inhalt()))) } else { Err(letzter_fehler()) }
    }

    #[test]
    fn seiten_holen_und_schicken() {
        let _allein = allein();
        // Der Server sagt zurück, was er bekommen hat.
        let port = probe_server(|anfrage| {
            let erste = anfrage.lines().next().unwrap_or("").to_string();
            let kopf = |name: &str| {
                anfrage.lines().find_map(|z| z.to_ascii_lowercase().strip_prefix(&format!("{name}:")).map(|w| w.trim().to_string())).unwrap_or_default()
            };
            let inhalt = anfrage.split_once("\r\n\r\n").map_or("", |(_, i)| i);
            let antwort = format!("{erste}|{}|{}|{inhalt}", kopf("x-probe"), kopf("content-type"));
            if erste.starts_with("GET /gibtsned") {
                einfach("404 Not Found", "", "nix do")
            } else {
                einfach("200 OK", "Content-Type: text/plain; charset=utf-8\r\nX-Zruck: jo\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n", &antwort)
            }
        });
        let basis = format!("http://127.0.0.1:{port}");
        assert_eq!(frag("GET", &format!("{basis}/a/b?x=grüß di"), &[], ""), Ok((200, "GET /a/b?x=gr%C3%BC%C3%9F%20di HTTP/1.1|||".to_string())));
        assert_eq!(
            frag("post", &format!("{basis}/schick"), &["X-Probe: servas", "Content-Type: application/json"], "{\"ä\": 1}"),
            Ok((200, "POST /schick HTTP/1.1|servas|application/json|{\"ä\": 1}".to_string()))
        );
        // Die Köpfe der Antwort: Namen klein, in der Reihenfolge, doppelte bleiben doppelt.
        let koepfe = oida_netz_antwort_koepfe();
        let koepfe: Vec<String> = unsafe {
            std::slice::from_raw_parts((*koepfe).daten as *const *mut TextKopf, (*koepfe).laenge as usize).iter().map(|t| inhalt(*t)).collect()
        };
        let paare: Vec<(&str, &str)> = koepfe.chunks(2).map(|p| (p[0].as_str(), p[1].as_str())).collect();
        assert!(paare.contains(&("x-zruck", "jo")) && paare.contains(&("content-type", "text/plain; charset=utf-8")), "{paare:?}");
        assert_eq!(paare.iter().filter(|(name, _)| *name == "set-cookie").map(|(_, w)| *w).collect::<Vec<_>>(), vec!["a=1", "b=2"]);
        // Ein Code wie 404 ist eine Antwort, kein Fehler.
        assert_eq!(frag("GET", &format!("{basis}/gibtsned"), &[], ""), Ok((404, "nix do".to_string())));
        assert_eq!(frag("DELETE", &format!("{basis}/weg"), &[], "").unwrap().1, "DELETE /weg HTTP/1.1|||");
        // Der Inhalt als Bytes und in eine Datei.
        assert_eq!(zahlen(oida_netz_antwort_bytes()).len(), "DELETE /weg HTTP/1.1|||".len());
        let datei = std::env::temp_dir().join(format!("oida-netz-probe-{}.txt", std::process::id()));
        unsafe { assert!(oida_netz_antwort_speichern(text(&datei.to_string_lossy()))) };
        assert_eq!(std::fs::read_to_string(&datei).unwrap(), "DELETE /weg HTTP/1.1|||");
        let _ = std::fs::remove_file(&datei);
    }

    #[test]
    fn umleitungen_stuecke_und_antworten_ohne_laenge() {
        let _allein = allein();
        let port = probe_server(|anfrage| {
            let erste = anfrage.lines().next().unwrap_or("");
            let pfad = erste.split_whitespace().nth(1).unwrap_or("");
            match pfad {
                "/oid" => b"HTTP/1.1 301 Moved Permanently\r\nLocation: /neich/do\r\nContent-Length: 0\r\n\r\n".to_vec(),
                "/neich/do" => b"HTTP/1.1 302 Found\r\nLocation: weida?x=1\r\nContent-Length: 0\r\n\r\n".to_vec(),
                "/neich/weida?x=1" => einfach("200 OK", "", &format!("ankumma mit {}", erste.split_whitespace().next().unwrap_or(""))),
                "/kreis" => b"HTTP/1.1 302 Found\r\nLocation: /kreis\r\nContent-Length: 0\r\n\r\n".to_vec(),
                // Die Längen der Stücke haben sechzehn Ziffern: "A" sind zehn Bytes.
                "/stickln" => b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nServa\r\n3;x=y\r\ns, \r\nA\r\nOIDA 12345\r\n0\r\nX-Hintn: jo\r\n\r\n".to_vec(),
                "/ohne" => b"HTTP/1.1 200 OK\r\n\r\nbis zum Schluss".to_vec(),
                "/laar" => b"HTTP/1.1 204 No Content\r\n\r\n".to_vec(),
                _ => b"des is ka HTTP\r\n\r\n".to_vec(),
            }
        });
        let basis = format!("http://127.0.0.1:{port}");
        // Zwei Umleitungen hintereinander, die zweite ohne / am Anfang.
        assert_eq!(frag("GET", &format!("{basis}/oid"), &[], ""), Ok((200, "ankumma mit GET".to_string())));
        // Nach einer Umleitung wird aus Schicken ein Holen.
        assert_eq!(frag("POST", &format!("{basis}/neich/do"), &[], "wos"), Ok((200, "ankumma mit GET".to_string())));
        let im_kreis = frag("GET", &format!("{basis}/kreis"), &[], "").unwrap_err();
        assert!(im_kreis.contains("im Kreis"), "{im_kreis}");
        assert_eq!(frag("GET", &format!("{basis}/stickln"), &[], ""), Ok((200, "Servas, OIDA 12345".to_string())));
        assert_eq!(frag("GET", &format!("{basis}/ohne"), &[], ""), Ok((200, "bis zum Schluss".to_string())));
        assert_eq!(frag("GET", &format!("{basis}/laar"), &[], ""), Ok((204, String::new())));
        assert!(frag("GET", &format!("{basis}/unsinn"), &[], "").is_err());
    }

    #[test]
    fn was_bei_einer_anfrage_schiefgehen_kann() {
        let _allein = allein();
        assert_eq!(frag("GET", "example.com", &[], ""), Err("De Adress 'example.com' muass mit http:// oda https:// aufaunga.".to_string()));
        assert!(frag("HOL MA", "http://127.0.0.1/", &[], "").unwrap_err().contains("is ka Oat vo Aufrog"));
        assert!(frag("GET", "http://127.0.0.1/", &["kaputt"], "").unwrap_err().contains("is ka Kopf"));
        // Ein Port, auf dem niemand horcht.
        let horcher = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = horcher.local_addr().unwrap().port();
        drop(horcher);
        let niemand = frag("GET", &format!("http://127.0.0.1:{port}/"), &[], "").unwrap_err();
        assert_eq!(niemand, format!("127.0.0.1 nimmt auf Port {port} kane Vabindungen au."));
        // Ein Rechner, den es sicher nicht gibt (die Endung .invalid ist dafür reserviert).
        assert_eq!(frag("GET", "http://den-gibts-ned.invalid/", &[], ""), Err("Den Rechner den-gibts-ned.invalid find i ned.".to_string()));
        // Ein Server, der nicht antwortet: Nach der Wartezeit ist Schluss.
        let stumm = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = stumm.local_addr().unwrap().port();
        oida_netz_wartezeit(1);
        let uhr = std::time::Instant::now();
        assert_eq!(frag("GET", &format!("http://127.0.0.1:{port}/"), &[], ""), Err("127.0.0.1 gibt ka Antwort.".to_string()));
        assert!(uhr.elapsed() < Duration::from_secs(10), "{:?}", uhr.elapsed());
        oida_netz_wartezeit(30);
        drop(stumm);
    }

    /// Braucht das Internet, darum nicht bei jedem Lauf dabei:
    /// `cargo test --release -p oida-laufzeit im_internet -- --ignored`
    #[test]
    #[ignore]
    fn im_internet_mit_https() {
        let _allein = allein();
        let (code, seite) = frag("GET", "https://example.com/", &[], "").unwrap();
        assert!(code == 200 && seite.contains("Example Domain"), "{code}: {seite}");
        // Von http wird auf https umgeleitet.
        let (code, _) = frag("GET", "http://github.com/", &[], "").unwrap();
        assert_eq!(code, 200);
        // Zertifikate, denen nicht zu trauen ist: abgelaufen, selbst ausgestellt, für einen anderen Namen.
        for kaputt in ["https://expired.badssl.com/", "https://self-signed.badssl.com/", "https://wrong.host.badssl.com/"] {
            // Diese Testseiten legen manchmal mitten im Aushandeln auf; dann noch einmal fragen.
            let mut meldung = frag("GET", kaputt, &[], "").unwrap_err();
            for _ in 0..5 {
                if !meldung.contains("is ogrissn") {
                    break;
                }
                meldung = frag("GET", kaputt, &[], "").unwrap_err();
            }
            assert!(meldung.starts_with("Dem Zertifikat vo "), "{kaputt}: {meldung}");
        }
    }
}
