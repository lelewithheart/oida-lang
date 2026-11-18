//! Macht aus einer MP3-Datei Töne zum Abspielen: 44100 Werte pro Sekunde,
//! zwei Kanäle (links, rechts, links, rechts ...), 16 Bit.
//!
//! Das Entpacken selbst macht Windows (der MP3-Baustein der "Audio
//! Compression Manager"-Schnittstelle, msacm32.dll). Hier wird nur die Datei
//! so weit gelesen, dass Windows weiß, was es bekommt, und am Ende
//! weggeschnitten, was beim Packen als Vor- und Nachlauf dazugekommen ist.
//! Sonst hätte ein Stück, das sich im Kreis wiederholen soll, eine Lücke.

use super::{GetProcAddress, LoadLibraryW, Zeiger};

/// Was am Anfang einer MP3-Datei über die Töne darin steht.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Kopf {
    /// Wo in der Datei das erste Stück mit Tönen beginnt.
    pub anfang: usize,
    pub rate: u32,
    pub kanaele: u16,
    /// Bits pro Sekunde
    pub bitrate: u32,
    /// So viele Bytes hat ein Stück (ohne Füllbyte), so viele Werte kommen heraus.
    pub stueck_bytes: u32,
    pub werte_pro_stueck: u32,
    /// Was beim Packen vorne und hinten dazugekommen ist (in Werten), wenn die Datei es verrät.
    pub vorlauf: Option<u32>,
    pub nachlauf: u32,
    /// Wie viele Stücke mit Tönen die Datei hat, wenn sie es verrät.
    pub stuecke: Option<u32>,
}

const BITRATEN_1: [u32; 15] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
const BITRATEN_2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];

/// Liest die vier Bytes am Anfang eines Stücks: (Fassung, Rate, Kanäle, Bitrate, Länge in Bytes, Werte).
fn stueck_kopf(b: &[u8]) -> Option<(u8, u32, u16, u32, u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] & 0xE0 != 0xE0 {
        return None;
    }
    let fassung = (b[1] >> 3) & 3; // 3: MPEG 1, 2: MPEG 2, 0: MPEG 2.5
    let schicht = (b[1] >> 1) & 3; // 1: Layer III
    let bitraten_stelle = (b[2] >> 4) as usize;
    let raten_stelle = ((b[2] >> 2) & 3) as usize;
    if fassung == 1 || schicht != 1 || bitraten_stelle == 0 || bitraten_stelle == 15 || raten_stelle == 3 {
        return None;
    }
    let fuellung = ((b[2] >> 1) & 1) as u32;
    let (rate, bitrate, werte) = match fassung {
        3 => ([44100, 48000, 32000][raten_stelle], BITRATEN_1[bitraten_stelle] * 1000, 1152),
        2 => ([22050, 24000, 16000][raten_stelle], BITRATEN_2[bitraten_stelle] * 1000, 576),
        _ => ([11025, 12000, 8000][raten_stelle], BITRATEN_2[bitraten_stelle] * 1000, 576),
    };
    let kanaele = if b[3] >> 6 == 3 { 1 } else { 2 };
    let laenge = werte / 8 * bitrate / rate + fuellung;
    Some((fassung, rate, kanaele, bitrate, laenge, werte))
}

/// Sucht das erste Stück mit Tönen und liest, was die Datei über sich sagt.
pub(super) fn kopf_lesen(daten: &[u8]) -> Option<Kopf> {
    let mut stelle = 0;
    // Davor kann ein Etikett stehen (Titel, Bild ...): "ID3", Fassung, Merker, Länge in vier Bytes zu 7 Bit.
    while daten.len() >= stelle + 10 && &daten[stelle..stelle + 3] == b"ID3" {
        let l = &daten[stelle + 6..stelle + 10];
        let laenge = ((l[0] as usize & 127) << 21) | ((l[1] as usize & 127) << 14) | ((l[2] as usize & 127) << 7) | (l[3] as usize & 127);
        stelle += 10 + laenge;
    }
    // Ein Stück gilt, wenn gleich dahinter wieder eines mit derselben Rate beginnt.
    let (fassung, rate, kanaele, bitrate, laenge, werte) = loop {
        let k = stueck_kopf(daten.get(stelle..)?);
        if let Some(k) = k {
            let danach = stelle + k.4 as usize;
            let naechstes = daten.get(danach..).and_then(stueck_kopf);
            if naechstes.is_some_and(|n| n.1 == k.1) || danach == daten.len() {
                break k;
            }
        }
        stelle += 1;
        if stelle > 1_000_000 {
            return None;
        }
    };
    let mut kopf = Kopf {
        anfang: stelle,
        rate,
        kanaele,
        bitrate,
        stueck_bytes: werte / 8 * bitrate / rate,
        werte_pro_stueck: werte,
        vorlauf: None,
        nachlauf: 0,
        stuecke: None,
    };
    // Das erste Stück ist oft ein Zettel vom Packer ("Xing" oder "Info") ohne Töne.
    let seite = match (fassung == 3, kanaele == 1) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    };
    let zettel = stelle + 4 + seite;
    if let Some(marke) = daten.get(zettel..zettel + 4)
        && (marke == b"Xing" || marke == b"Info")
    {
        kopf.anfang = stelle + laenge as usize;
        let merker = u32::from_be_bytes(daten.get(zettel + 4..zettel + 8)?.try_into().ok()?);
        let mut weiter = zettel + 8;
        if merker & 1 != 0 {
            kopf.stuecke = Some(u32::from_be_bytes(daten.get(weiter..weiter + 4)?.try_into().ok()?));
            weiter += 4;
        }
        weiter += if merker & 2 != 0 { 4 } else { 0 };
        weiter += if merker & 4 != 0 { 100 } else { 0 };
        weiter += if merker & 8 != 0 { 4 } else { 0 };
        // Dahinter schreibt LAME (und ffmpeg): 9 Zeichen Name, dann 12 Bytes, dann je 12 Bit Vor- und Nachlauf.
        if let Some(lame) = daten.get(weiter..weiter + 24)
            && (lame.starts_with(b"LAME") || lame.starts_with(b"Lavc") || lame.starts_with(b"Lavf"))
        {
            let b = &lame[21..24];
            let vorlauf = ((b[0] as u32) << 4) | (b[1] as u32 >> 4);
            let nachlauf = ((b[1] as u32 & 15) << 8) | b[2] as u32;
            if vorlauf > 0 || nachlauf > 0 {
                kopf.vorlauf = Some(vorlauf);
                kopf.nachlauf = nachlauf;
            }
        }
    }
    Some(kopf)
}

// Die Strukturen von Windows sind hier ohne Lücken gepackt (pshpack1.h).
#[repr(C, packed)]
struct TonArt {
    art: u16,
    kanaele: u16,
    rate: u32,
    bytes_pro_sekunde: u32,
    block: u16,
    bits: u16,
    extra: u16,
}

#[repr(C, packed)]
struct Mp3Art {
    grund: TonArt,
    kennung: u16,
    merker: u32,
    stueck_bytes: u16,
    stuecke_pro_block: u16,
    verzug: u16,
}

#[repr(C, packed)]
struct WandelKopf {
    groesse: u32,
    zustand: u32,
    eigenes: usize,
    quelle: *const u8,
    quelle_laenge: u32,
    quelle_gebraucht: u32,
    quelle_eigenes: usize,
    ziel: *mut u8,
    ziel_laenge: u32,
    ziel_gebraucht: u32,
    ziel_eigenes: usize,
    reserviert: [u32; 15],
}

unsafe fn acm<F: Copy>(name: &[u8]) -> Option<F> {
    unsafe {
        let dll: Vec<u16> = "msacm32.dll".encode_utf16().chain(std::iter::once(0)).collect();
        let b = LoadLibraryW(dll.as_ptr());
        if b.is_null() {
            return None;
        }
        let f = GetProcAddress(b, name.as_ptr());
        if f.is_null() { None } else { Some(std::mem::transmute_copy::<Zeiger, F>(&f)) }
    }
}

/// Lässt Windows die Stücke entpacken: heraus kommen die Werte, wie sie in der Datei stehen
/// (in ihrer Rate, mit ihren Kanälen), mit dem Vorlauf vom Packen noch dran.
fn roh_entpacken(daten: &[u8], kopf: &Kopf) -> Result<Vec<i16>, String> {
    type Auf = unsafe extern "system" fn(*mut Zeiger, Zeiger, *const Mp3Art, *const TonArt, Zeiger, usize, usize, u32) -> u32;
    type Groesse = unsafe extern "system" fn(Zeiger, u32, *mut u32, u32) -> u32;
    type Kopfarbeit = unsafe extern "system" fn(Zeiger, *mut WandelKopf, u32) -> u32;
    type Zu = unsafe extern "system" fn(Zeiger, u32) -> u32;
    let fehlt = || "Windows hot do kan Baustein fia MP3.".to_string();
    unsafe {
        let auf: Auf = acm(b"acmStreamOpen\0").ok_or_else(fehlt)?;
        let groesse: Groesse = acm(b"acmStreamSize\0").ok_or_else(fehlt)?;
        let vorbereiten: Kopfarbeit = acm(b"acmStreamPrepareHeader\0").ok_or_else(fehlt)?;
        let wandeln: Kopfarbeit = acm(b"acmStreamConvert\0").ok_or_else(fehlt)?;
        let aufraeumen: Kopfarbeit = acm(b"acmStreamUnprepareHeader\0").ok_or_else(fehlt)?;
        let zu: Zu = acm(b"acmStreamClose\0").ok_or_else(fehlt)?;

        let quelle = Mp3Art {
            grund: TonArt {
                art: 0x55, // MPEG Layer 3
                kanaele: kopf.kanaele,
                rate: kopf.rate,
                bytes_pro_sekunde: kopf.bitrate / 8,
                block: 1,
                bits: 0,
                extra: 12,
            },
            kennung: 1,
            merker: 2,
            stueck_bytes: kopf.stueck_bytes as u16,
            stuecke_pro_block: 1,
            verzug: 0,
        };
        let ziel = TonArt {
            art: 1, // ungepackt
            kanaele: kopf.kanaele,
            rate: kopf.rate,
            bytes_pro_sekunde: kopf.rate * kopf.kanaele as u32 * 2,
            block: kopf.kanaele * 2,
            bits: 16,
            extra: 0,
        };
        let mut strom: Zeiger = std::ptr::null_mut();
        if auf(&mut strom, std::ptr::null_mut(), &quelle, &ziel, std::ptr::null_mut(), 0, 0, 0) != 0 {
            return Err(fehlt());
        }
        // In Häppchen: so viel hinein, und Platz für das, was Windows dafür höchstens herausgibt.
        const HAPPEN: usize = 64 * 1024;
        let mut ziel_bytes = 0u32;
        if groesse(strom, HAPPEN as u32, &mut ziel_bytes, 0) != 0 || ziel_bytes == 0 {
            zu(strom, 0);
            return Err("Windows kaun des MP3 ned entpockn.".to_string());
        }
        let mut hinein = vec![0u8; HAPPEN];
        let mut heraus = vec![0u8; ziel_bytes as usize];
        let mut wk: WandelKopf = std::mem::zeroed();
        wk.groesse = std::mem::size_of::<WandelKopf>() as u32;
        wk.quelle = hinein.as_ptr();
        wk.quelle_laenge = HAPPEN as u32;
        wk.ziel = heraus.as_mut_ptr();
        wk.ziel_laenge = ziel_bytes;
        if vorbereiten(strom, &mut wk, 0) != 0 {
            zu(strom, 0);
            return Err("Windows kaun des MP3 ned entpockn.".to_string());
        }

        // Hinten kann noch ein altes Etikett hängen ("TAG", 128 Bytes).
        let mut ende = daten.len();
        if ende >= 128 && &daten[ende - 128..ende - 125] == b"TAG" {
            ende -= 128;
        }
        let mut werte: Vec<i16> = Vec::new();
        let mut stelle = kopf.anfang;
        let mut rest = 0usize; // was vom letzten Happen noch nicht verbraucht ist
        let mut erster = true;
        let mut gut = true;
        loop {
            let neu = (ende - stelle).min(HAPPEN - rest);
            hinein[rest..rest + neu].copy_from_slice(&daten[stelle..stelle + neu]);
            stelle += neu;
            let da = rest + neu;
            let letzter = stelle >= ende;
            wk.quelle_laenge = da as u32;
            wk.quelle_gebraucht = 0;
            wk.ziel_gebraucht = 0;
            // 4: nur ganze Stücke. 0x10: der Anfang. 0x20: das Ende (dann kommt auch der Rest heraus).
            let merker = if letzter { 0x20 } else { 4 } | if erster { 0x10 } else { 0 };
            erster = false;
            if wandeln(strom, &mut wk, merker) != 0 {
                gut = false;
                break;
            }
            let gebraucht = wk.quelle_gebraucht as usize;
            let bekommen = wk.ziel_gebraucht as usize;
            werte.extend(heraus[..bekommen].chunks_exact(2).map(|p| i16::from_le_bytes([p[0], p[1]])));
            rest = da - gebraucht.min(da);
            hinein.copy_within(gebraucht.min(da)..da, 0);
            if letzter || (gebraucht == 0 && bekommen == 0) {
                break;
            }
        }
        wk.quelle_laenge = HAPPEN as u32;
        aufraeumen(strom, &mut wk, 0);
        zu(strom, 0);
        if !gut || werte.is_empty() {
            return Err("Windows kaun des MP3 ned entpockn.".to_string());
        }
        Ok(werte)
    }
}

/// Aus dem, was Windows liefert, wird das, was der Mischer spielt: ohne Vor- und
/// Nachlauf, zwei Kanäle, 44100 Werte pro Sekunde.
pub(super) fn zuschneiden(roh: &[i16], kopf: &Kopf, verzug: usize) -> Vec<i16> {
    let kanaele = kopf.kanaele as usize;
    let alle = roh.len() / kanaele;
    // Vorne: der Vorlauf vom Packen und der vom Entpacken. Hinten: bis zur echten Länge.
    let vorne = (kopf.vorlauf.unwrap_or(VORLAUF_UEBLICH) as usize + verzug).min(alle);
    let echt = match (kopf.stuecke, kopf.vorlauf) {
        (Some(stuecke), Some(vorlauf)) => {
            (stuecke as usize * kopf.werte_pro_stueck as usize).saturating_sub(vorlauf as usize + kopf.nachlauf as usize)
        }
        _ => alle - vorne,
    };
    let bis = (vorne + echt).min(alle);
    let teil = &roh[vorne * kanaele..bis * kanaele];
    let links_rechts = |i: usize| -> (f32, f32) {
        if kanaele == 1 { (teil[i] as f32, teil[i] as f32) } else { (teil[i * 2] as f32, teil[i * 2 + 1] as f32) }
    };
    let laenge = teil.len() / kanaele;
    if kopf.rate == 44100 {
        let mut aus = Vec::with_capacity(laenge * 2);
        for i in 0..laenge {
            let (l, r) = links_rechts(i);
            aus.push(l as i16);
            aus.push(r as i16);
        }
        return aus;
    }
    // Andere Rate: zwischen den Werten geradlinig umrechnen.
    let neu = (laenge as u64 * 44100 / kopf.rate as u64) as usize;
    let mut aus = Vec::with_capacity(neu * 2);
    for i in 0..neu {
        let genau = i as f64 * kopf.rate as f64 / 44100.0;
        let a = (genau as usize).min(laenge.saturating_sub(1));
        let b = (a + 1).min(laenge.saturating_sub(1));
        let anteil = (genau - a as f64) as f32;
        let ((l1, r1), (l2, r2)) = (links_rechts(a), links_rechts(b));
        aus.push((l1 + (l2 - l1) * anteil) as i16);
        aus.push((r1 + (r2 - r1) * anteil) as i16);
    }
    aus
}

/// Der Verzug, den das Entpacken von MP3 immer dazugibt (in Werten). Gemessen
/// gegen ffmpeg: Mit Vorlauf vom Packen (576 bei LAME) plus diesen 529 liegt
/// das Ergebnis von Windows auf den Wert genau auf dem von ffmpeg.
pub(super) const VERZUG: usize = 529;
/// Verrät die Datei ihren Vorlauf nicht, gilt der übliche Wert von LAME.
const VORLAUF_UEBLICH: u32 = 576;

/// Liest eine MP3-Datei und gibt die Töne zum Abspielen zurück.
pub(super) fn entpacken(daten: &[u8]) -> Result<Vec<i16>, String> {
    let kopf = kopf_lesen(daten).ok_or_else(|| "Des is ka MP3, des i lesn kaun.".to_string())?;
    let roh = roh_entpacken(daten, &kopf)?;
    Ok(zuschneiden(&roh, &kopf, VERZUG))
}

/// Für die Prüfung von außen: die Werte, wie Windows sie liefert, und der Kopf dazu.
#[cfg(test)]
pub(super) fn roh_fuer_test(daten: &[u8]) -> Option<(Vec<i16>, Kopf)> {
    let kopf = kopf_lesen(daten)?;
    roh_entpacken(daten, &kopf).ok().map(|roh| (roh, kopf))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kein Test, ein Werkzeug: `OIDA_MP3_PROBE=ein.mp3;aus.roh cargo test -p oida-laufzeit mp3_probe -- --ignored`
    /// schreibt, was Windows aus der Datei macht (16 Bit, roh), und den Kopf dazu.
    #[test]
    #[ignore]
    fn mp3_probe() {
        let angabe = std::env::var("OIDA_MP3_PROBE").unwrap();
        let (ein, aus) = angabe.split_once(';').unwrap();
        let daten = std::fs::read(ein).unwrap();
        let (roh, kopf) = roh_fuer_test(&daten).unwrap();
        println!("{kopf:?} roh: {} Werte pro Kanal", roh.len() / kopf.kanaele as usize);
        let bytes: Vec<u8> = roh.iter().flat_map(|w| w.to_le_bytes()).collect();
        std::fs::write(aus, bytes).unwrap();
        let fertig = entpacken(&daten).unwrap();
        println!("fertig: {} Werte pro Kanal", fertig.len() / 2);
    }

    #[test]
    fn strukturen_wie_bei_windows() {
        assert_eq!(std::mem::size_of::<TonArt>(), 18);
        assert_eq!(std::mem::size_of::<Mp3Art>(), 30);
        assert_eq!(std::mem::size_of::<WandelKopf>(), 124);
    }

    #[test]
    fn stueck_koepfe() {
        // MPEG 1, Layer III, 160 kbit/s, 44100 Hz, zwei Kanäle, ohne Füllbyte: 522 Bytes, 1152 Werte
        assert_eq!(stueck_kopf(&[0xFF, 0xFB, 0xA0, 0x00]), Some((3, 44100, 2, 160_000, 522, 1152)));
        // ... mit Füllbyte und nur einem Kanal
        assert_eq!(stueck_kopf(&[0xFF, 0xFB, 0xA2, 0xC0]), Some((3, 44100, 1, 160_000, 523, 1152)));
        // MPEG 2, 64 kbit/s, 22050 Hz
        assert_eq!(stueck_kopf(&[0xFF, 0xF3, 0x80, 0x00]), Some((2, 22050, 2, 64_000, 208, 576)));
        // Kein Stück: falscher Anfang, Layer II, verbotene Rate
        assert_eq!(stueck_kopf(&[0x49, 0x44, 0x33, 0x04]), None);
        assert_eq!(stueck_kopf(&[0xFF, 0xFD, 0xA0, 0x00]), None);
        assert_eq!(stueck_kopf(&[0xFF, 0xFB, 0xAC, 0x00]), None);
    }

    #[test]
    fn kopf_mit_etikett_und_zettel() {
        // Ein Etikett von 20 Bytes, dann ein Zettel-Stück ("Info" mit Anzahl 100 und LAME-Angaben), dann Töne.
        let mut d = b"ID3\x04\x00\x00\x00\x00\x00\x0a".to_vec();
        d.extend([0u8; 10]);
        let zettel_anfang = d.len();
        d.extend([0xFF, 0xFB, 0xA0, 0x00]);
        d.extend([0u8; 32]);
        d.extend(b"Info");
        d.extend(1u32.to_be_bytes());
        d.extend(100u32.to_be_bytes());
        let mut lame = b"LAME3.100".to_vec();
        lame.extend([0u8; 12]);
        // Vorlauf 576 (0x240), Nachlauf 1000 (0x3E8): 0x24, 0x03, 0xE8
        lame.extend([0x24, 0x03, 0xE8]);
        d.extend(lame);
        d.resize(zettel_anfang + 522, 0);
        for _ in 0..3 {
            d.extend([0xFF, 0xFB, 0xA0, 0x00]);
            d.extend([0u8; 518]);
        }
        let kopf = kopf_lesen(&d).unwrap();
        assert_eq!(kopf.anfang, zettel_anfang + 522);
        assert_eq!((kopf.rate, kopf.kanaele, kopf.bitrate), (44100, 2, 160_000));
        assert_eq!((kopf.vorlauf, kopf.nachlauf, kopf.stuecke), (Some(576), 1000, Some(100)));
        assert_eq!(kopf_lesen(b"des is ka mp3"), None);
    }

    #[test]
    fn zuschneiden_schneidet_vorne_und_hinten() {
        let kopf = Kopf {
            anfang: 0,
            rate: 44100,
            kanaele: 1,
            bitrate: 128_000,
            stueck_bytes: 417,
            werte_pro_stueck: 1152,
            vorlauf: Some(10),
            nachlauf: 2294,
            stuecke: Some(2),
        };
        // 2 Stücke = 2304 Werte, davon echt: 2304 - 10 - 2294 = 0 ... also lieber ein Stück mehr.
        let kopf3 = Kopf { stuecke: Some(3), ..kopf };
        let roh: Vec<i16> = (0..4000).map(|i| i as i16).collect();
        let aus = zuschneiden(&roh, &kopf3, 5);
        // Echt sind 3 * 1152 - 10 - 2294 = 1152 Werte, ab dem 15. (Vorlauf 10 + Verzug 5), auf zwei Kanäle verdoppelt.
        assert_eq!(aus.len(), 1152 * 2);
        assert_eq!((aus[0], aus[1], aus[2]), (15, 15, 16));
        // Halbe Rate: doppelt so viele Werte heraus.
        let langsam = Kopf { rate: 22050, vorlauf: Some(0), stuecke: None, ..kopf };
        let aus = zuschneiden(&roh[..100], &langsam, 0);
        assert_eq!(aus.len(), 200 * 2);
        assert_eq!((aus[0], aus[2], aus[4]), (0, 0, 1));
        // Ohne Angabe in der Datei gilt der übliche Vorlauf.
        let ohne = Kopf { vorlauf: None, stuecke: None, ..kopf };
        let aus = zuschneiden(&roh, &ohne, 529);
        assert_eq!(aus[0], 576 + 529);
        assert_eq!(aus.len(), (4000 - 1105) * 2);

        // Eine echte Datei (von ffmpeg/LAME gepackt): eine Sekunde, 0,2 s Stille,
        // dann 0,6 s lang links 440 und rechts 660 Hertz, dann Stille.
        let fertig = entpacken(include_bytes!("ton_test.mp3")).unwrap();
        assert_eq!(fertig.len(), 44100 * 2);
        let links = |i: usize| fertig[i * 2] as f64;
        let rechts = |i: usize| fertig[i * 2 + 1] as f64;
        assert!((0..8000).all(|i| links(i).abs() < 300.0 && rechts(i).abs() < 300.0));
        assert!((36000..44100).all(|i| links(i).abs() < 300.0));
        // Mittendrin stimmt jeder Wert mit der gedachten Schwingung überein (bis auf das Rauschen vom Packen).
        for i in 12000..32000 {
            let t = (i - 8820) as f64 / 44100.0;
            let soll_l = 12000.0 * (std::f64::consts::TAU * 440.0 * t).sin();
            let soll_r = 12000.0 * (std::f64::consts::TAU * 660.0 * t).sin();
            assert!((links(i) - soll_l).abs() < 800.0, "links bei {i}: {} statt {soll_l}", links(i));
            assert!((rechts(i) - soll_r).abs() < 800.0, "rechts bei {i}: {} statt {soll_r}", rechts(i));
        }
    }
}
