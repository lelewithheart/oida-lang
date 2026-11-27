//! `oida hol`: Pakete holen, also Bibliotheken anderer Leute, die auf GitHub liegen.
//!
//! ```text
//! oida hol wer/wüafl             das Paket von github.com/wer/wüafl holen
//! oida hol wer/wüafl@v1.2        einen bestimmten Stand (Marke, Zweig oder Commit)
//! oida hol ../mei-paket          einen Ordner am eigenen Rechner als Paket nehmen
//! oida hol wer/wüafl --ois w     unter einem anderen Namen ablegen
//! oida hol                       alle Pakete aus pakete.txt wieder holen
//! oida hol --neich               ... und dabei den neuesten Stand nehmen
//! ```
//!
//! Ein Paket landet im Ordner `pakete/<name>` neben dem eigenen Programm und wird in
//! `pakete.txt` eingetragen, samt einer Prüfsumme über seinen Inhalt. Im Programm holt man es
//! mit `BRAUCH "<name>" OIS <name> OIDA` dazu (siehe [`crate::treiber::finde_brauch`]).
//!
//! Was ein Paket ist: ein Ordner, in dem `<name>.oida` oder `paket.oida` liegt. Von GitHub
//! kommt er als Archiv (`.tar.gz`); zum Laden und Auspacken werden `curl` und `tar` verwendet,
//! die bei Windows 10 und 11 und bei Linux dabei sind. Hat ein Paket selbst eine `pakete.txt`,
//! wird auch geholt, was es braucht.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::sha256::Sha256;

/// Der Ordner neben dem Programm, in dem die Pakete liegen.
pub const ORDNER: &str = "pakete";
/// Die Liste der Pakete: je Zeile `name = woher sha256:prüfsumme`.
pub const LISTE: &str = "pakete.txt";

/// Ein Eintrag in `pakete.txt`.
#[derive(Debug, Clone, PartialEq)]
struct Eintrag {
    /// Woher das Paket kommt, so wie es angegeben wurde: `wer/wüafl@v1.2` oder ein Ordner.
    woher: String,
    /// Die Prüfsumme über den Inhalt, als es zuletzt geholt wurde.
    summe: String,
}

/// Woher ein Paket kommt.
#[derive(Debug, Clone, PartialEq)]
enum Quelle {
    /// `wer/repo` auf GitHub, dazu der Stand (Marke, Zweig oder Commit), wenn einer angegeben ist.
    Github { wer: String, repo: String, stand: Option<String> },
    /// Ein Ordner am eigenen Rechner.
    Ordner(PathBuf),
}

/// Liest, was hinter `oida hol` steht, und erledigt es im Ordner, in dem man gerade ist.
pub fn hol(argumente: &[String]) -> Result<(), String> {
    let mut adresse: Option<&str> = None;
    let mut name: Option<String> = None;
    let mut neu = false;
    let mut i = 0;
    while i < argumente.len() {
        match argumente[i].as_str() {
            "--ois" => {
                i += 1;
                name = Some(argumente.get(i).ok_or("Noch --ois fehlt da Nom.")?.clone());
            }
            "--neich" => neu = true,
            a if a.starts_with("--") => return Err(format!("De Option {a} kenn i bei 'hol' ned. Es gibt: --ois <nom>, --neich.")),
            a if adresse.is_some() => return Err(format!("Ans noch'm aundan: I hoi nur a Paket auf amoi ('{a}' is z'vü).")),
            a => adresse = Some(a),
        }
        i += 1;
    }
    let hier = PathBuf::from(".");
    let mut liste = liste_lesen(&hier)?;
    match adresse {
        Some(adresse) => {
            let (quelle, vorschlag) = quelle_lesen(adresse)?;
            let name = name.unwrap_or(vorschlag);
            name_pruefen(&name)?;
            let mut geholt = Vec::new();
            holen(&hier, &name, adresse, &quelle, None, &mut liste, &mut geholt)?;
            liste_schreiben(&hier, &liste)?;
            println!("Passt! '{name}' liegt in {ORDNER}/{name}. Im Programm: BRAUCH \"{name}\" OIS {name} OIDA");
        }
        None => {
            if name.is_some() {
                return Err("--ois geht nur, wenn dabeisteht, wöchs Paket gmant is.".to_string());
            }
            if liste.is_empty() {
                return Err(format!(
                    "Wos soi i hoin? Schreib: oida hol wer/paket   (oda leg a {LISTE} au; do steht derweil nix drin)."
                ));
            }
            // Alles aus der Liste wieder holen. Ohne --neich muss dabei genau das herauskommen,
            // was zuletzt geholt wurde: Sonst hat jemand das Paket geändert.
            let mut geholt = Vec::new();
            for (name, eintrag) in liste.clone() {
                if geholt.contains(&name) {
                    continue;
                }
                let (quelle, _) = quelle_lesen(&eintrag.woher)?;
                let erwartet = if neu { None } else { Some(eintrag.summe.as_str()) };
                holen(&hier, &name, &eintrag.woher, &quelle, erwartet, &mut liste, &mut geholt)?;
            }
            liste_schreiben(&hier, &liste)?;
            println!("Passt! {} da: {}.", if geholt.len() == 1 { "A Paket is" } else { "De Pakete san" }, geholt.join(", "));
        }
    }
    Ok(())
}

/// Macht aus einer Angabe wie `wer/wüafl@v1.2`, `https://github.com/wer/wüafl` oder `../ordner`
/// die Quelle und den Namen, den das Paket bekommt, wenn nichts anderes verlangt ist.
fn quelle_lesen(adresse: &str) -> Result<(Quelle, String), String> {
    let pfad = Path::new(adresse);
    let sieht_wie_pfad = adresse.starts_with('.') || adresse.starts_with('/') || adresse.contains('\\') || adresse.contains(':') && !adresse.contains("://");
    // Ein Ordner am eigenen Rechner fängt mit . oder / an (oder ist ein Windows-Pfad); alles andere ist wer/paket.
    if sieht_wie_pfad {
        if !pfad.is_dir() {
            return Err(format!("Den Ordner {adresse} find i ned."));
        }
        let voll = fs::canonicalize(pfad).unwrap_or_else(|_| pfad.to_path_buf());
        let name = voll.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return Ok((Quelle::Ordner(pfad.to_path_buf()), name));
    }
    let ohne = adresse
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_start_matches("github.com/")
        .trim_end_matches('/');
    let (ort, stand) = match ohne.split_once('@') {
        Some((ort, stand)) if !stand.is_empty() => (ort, Some(stand.to_string())),
        _ => (ohne, None),
    };
    let ort = ort.trim_end_matches(".git");
    let teile: Vec<&str> = ort.split('/').collect();
    let erlaubt = |t: &str| !t.is_empty() && t != "." && t != ".." && t.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c));
    if teile.len() != 2 || !teile.iter().all(|t| erlaubt(t)) || stand.as_deref().is_some_and(|s| !s.chars().all(|c| c.is_alphanumeric() || "-_./".contains(c)) || s.contains(".."))
    {
        return Err(format!(
            "'{adresse}' vasteh i ned. So geht's: oida hol wer/paket   (des is github.com/wer/paket), \
             mit an bestimmtn Staund: wer/paket@v1.2, oda a Ordner: oida hol ../mei-paket"
        ));
    }
    Ok((Quelle::Github { wer: teile[0].to_string(), repo: teile[1].to_string(), stand }, teile[1].to_string()))
}

/// Ein Paketname wird ein Ordnername und steht im Programm hinter OIS: nur, was dort überall geht.
fn name_pruefen(name: &str) -> Result<(), String> {
    let passt = !name.is_empty() && !name.starts_with('.') && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-');
    if passt { Ok(()) } else { Err(format!("'{name}' geht ned ois Nom fia a Paket: nur Buchstobn, Ziffan, '_' und '-'.")) }
}

/// Holt ein Paket in `<wo>/pakete/<name>` und trägt es in die Liste ein. Steht `erwartet` da,
/// muss der Inhalt diese Prüfsumme haben. Was das Paket selbst braucht, wird auch geholt.
fn holen(
    wo: &Path,
    name: &str,
    woher: &str,
    quelle: &Quelle,
    erwartet: Option<&str>,
    liste: &mut BTreeMap<String, Eintrag>,
    geholt: &mut Vec<String>,
) -> Result<(), String> {
    if geholt.len() > 200 {
        return Err("Des san z'vü Pakete, de si gegnseitig brauchn. Do stimmt wos ned.".to_string());
    }
    let arbeit = arbeitsordner()?;
    let ergebnis = (|| {
        let inhalt = match quelle {
            Quelle::Ordner(ordner) => ordner.clone(),
            Quelle::Github { wer, repo, stand } => {
                let basis = std::env::var("OIDA_PAKET_QUELLE").ok().filter(|q| !q.is_empty());
                let adresse = format!(
                    "{}/{wer}/{repo}/archive/{}.tar.gz",
                    basis.as_deref().unwrap_or("https://github.com").trim_end_matches('/'),
                    stand.as_deref().unwrap_or("HEAD")
                );
                let archiv = arbeit.join("paket.tar.gz");
                println!("Hoi {adresse} ...");
                // Nur https (und file, wenn zum Testen eine andere Quelle eingestellt ist).
                laden(&adresse, &archiv, basis.is_some())?;
                let ausgepackt = arbeit.join("aus");
                fs::create_dir_all(&ausgepackt).map_err(|e| format!("I kaun {} ned aulegn: {e}", ausgepackt.display()))?;
                auspacken(&archiv, &ausgepackt)?;
                // GitHub legt alles in einen Ordner "repo-stand": Der Inhalt davon ist das Paket.
                let drin: Vec<PathBuf> = fs::read_dir(&ausgepackt)
                    .map_err(|e| format!("I kaun {} ned lesn: {e}", ausgepackt.display()))?
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .collect();
                match drin.as_slice() {
                    [einer] if einer.is_dir() => einer.clone(),
                    _ => ausgepackt,
                }
            }
        };
        if eingang(&inhalt, name).is_none() {
            return Err(format!(
                "Des is ka Paket fia OIDA: Do drin gibt's ka '{name}.oida' und ka 'paket.oida'. (Gschaut hob i in {woher}.)"
            ));
        }
        // Erst daneben ablegen und prüfen, dann das alte ersetzen: So bleibt nie ein halbes Paket liegen.
        let neu = arbeit.join("neu");
        kopier_ordner(&inhalt, &neu)?;
        let summe = pruefsumme(&neu)?;
        if let Some(erwartet) = erwartet
            && erwartet != summe
        {
            return Err(format!(
                "Des Paket '{name}' is nimma desöwe wia beim letztn Moi: De Prüfsumme is aundas.\n  \
                 in {LISTE}:  {erwartet}\n  jetzt:         {summe}\n\
                 Wüst den neichn Staund hom, daun: oida hol --neich"
            ));
        }
        let ziel = wo.join(ORDNER).join(name);
        if ziel.exists() {
            fs::remove_dir_all(&ziel).map_err(|e| format!("I kaun des oide {} ned wegtuan: {e}", ziel.display()))?;
        }
        kopier_ordner(&neu, &ziel)?;
        liste.insert(name.to_string(), Eintrag { woher: woher.to_string(), summe });
        geholt.push(name.to_string());

        // Was das Paket selbst braucht: seine eigene Liste. Was es hier schon gibt, bleibt.
        for (innen_name, eintrag) in liste_lesen(&ziel)? {
            if liste.contains_key(&innen_name) || geholt.contains(&innen_name) {
                continue;
            }
            let (innen_quelle, _) = quelle_lesen(&eintrag.woher)?;
            // Ein Ordner in der Liste eines Pakets wäre ein Ordner auf dem Rechner von wem anderen.
            if matches!(innen_quelle, Quelle::Ordner(_)) {
                return Err(format!(
                    "Des Paket '{name}' braucht '{innen_name}' aus an Ordner ({}), den's nur beim Schreiba gibt. Des kaun i ned hoin.",
                    eintrag.woher
                ));
            }
            holen(wo, &innen_name, &eintrag.woher, &innen_quelle, Some(&eintrag.summe), liste, geholt)?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&arbeit);
    ergebnis
}

/// Die Datei, mit der ein Paket anfängt: `<name>.oida`, sonst `paket.oida`.
pub fn eingang(ordner: &Path, name: &str) -> Option<PathBuf> {
    [format!("{name}.oida"), "paket.oida".to_string()].into_iter().map(|d| ordner.join(d)).find(|d| d.is_file())
}

fn arbeitsordner() -> Result<PathBuf, String> {
    let zeit = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let ordner = std::env::temp_dir().join("oida").join(format!("paket-{}-{zeit}", std::process::id()));
    fs::create_dir_all(&ordner).map_err(|e| format!("I kaun {} ned aulegn: {e}", ordner.display()))?;
    Ok(ordner)
}

/// Ein Programm des Systems. Unter Windows das aus System32 (nicht irgendeines aus dem PATH).
fn werkzeug(name: &str) -> Command {
    #[cfg(windows)]
    if let Some(wurzel) = std::env::var_os("SystemRoot") {
        let fest = PathBuf::from(wurzel).join("System32").join(format!("{name}.exe"));
        if fest.is_file() {
            return Command::new(fest);
        }
    }
    Command::new(name)
}

fn laden(adresse: &str, ziel: &Path, auch_datei: bool) -> Result<(), String> {
    let lauf = werkzeug("curl")
        .args(["--fail", "--silent", "--show-error", "--location", "--max-time", "300", "--max-filesize", "209715200"])
        .args(["--proto", if auch_datei { "=https,file" } else { "=https" }])
        .arg("--output")
        .arg(ziel)
        .arg(adresse)
        .output()
        .map_err(|e| format!("Zum Hoin brauch i 'curl', des find i oba ned: {e}"))?;
    if lauf.status.success() {
        return Ok(());
    }
    let grund = String::from_utf8_lossy(&lauf.stderr).trim().to_string();
    Err(if grund.contains("404") {
        format!("Des gibt's ned: {adresse}\nStimmt da Nom, und is des Paket öffentlich?")
    } else {
        format!("Des Hoin hot ned klappt: {adresse}\n{grund}")
    })
}

fn auspacken(archiv: &Path, ziel: &Path) -> Result<(), String> {
    let lauf = werkzeug("tar")
        .arg("-xzf")
        .arg(archiv)
        .arg("-C")
        .arg(ziel)
        .output()
        .map_err(|e| format!("Zum Auspockn brauch i 'tar', des find i oba ned: {e}"))?;
    if lauf.status.success() {
        Ok(())
    } else {
        Err(format!("Des Paket lasst si ned auspockn: {}", String::from_utf8_lossy(&lauf.stderr).trim()))
    }
}

/// Alle Dateien eines Ordners (nicht die Verweise auf andere Dateien), mit dem Pfad ab dem
/// Ordner und `/` dazwischen, sortiert. Was nicht zum Paket gehört, bleibt draußen.
fn dateien(ordner: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    fn sammle(ordner: &Path, vorne: &str, aus: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
        for eintrag in fs::read_dir(ordner)? {
            let eintrag = eintrag?;
            let name = eintrag.file_name().to_string_lossy().into_owned();
            let art = eintrag.file_type()?;
            // Verweise (Symlinks) könnten aus dem Paket hinaus zeigen.
            if art.is_symlink() {
                continue;
            }
            if art.is_dir() {
                // Die Pakete eines Pakets werden selbst geholt, und was beim Bauen entsteht, gehört nicht dazu.
                if vorne.is_empty() && (name == ".git" || name == "target" || name == ORDNER) {
                    continue;
                }
                sammle(&eintrag.path(), &format!("{vorne}{name}/"), aus)?;
            } else if art.is_file() {
                aus.push((format!("{vorne}{name}"), eintrag.path()));
            }
        }
        Ok(())
    }
    let mut aus = Vec::new();
    sammle(ordner, "", &mut aus).map_err(|e| format!("I kaun {} ned lesn: {e}", ordner.display()))?;
    aus.sort();
    Ok(aus)
}

fn kopier_ordner(von: &Path, nach: &Path) -> Result<(), String> {
    for (name, quelle) in dateien(von)? {
        let ziel = nach.join(&name);
        let fehler = |e: std::io::Error| format!("I kaun {} ned schreibn: {e}", ziel.display());
        fs::create_dir_all(ziel.parent().unwrap_or(nach)).map_err(fehler)?;
        fs::copy(&quelle, &ziel).map_err(fehler)?;
    }
    Ok(())
}

/// Die Prüfsumme über ein Paket: die Namen und Inhalte aller Dateien, in fester Reihenfolge.
/// Zeilenenden zählen nicht mit (`\r\n` wie `\n`): So kommt unter Windows und Linux dasselbe heraus.
fn pruefsumme(ordner: &Path) -> Result<String, String> {
    let mut summe = Sha256::neu();
    for (name, datei) in dateien(ordner)? {
        let inhalt = fs::read(&datei).map_err(|e| format!("I kaun {} ned lesn: {e}", datei.display()))?;
        let inhalt: Vec<u8> = if inhalt.contains(&0) {
            inhalt // keine Textdatei: genau so, wie sie ist
        } else {
            let mut ohne = Vec::with_capacity(inhalt.len());
            for (i, b) in inhalt.iter().enumerate() {
                if !(*b == b'\r' && inhalt.get(i + 1) == Some(&b'\n')) {
                    ohne.push(*b);
                }
            }
            ohne
        };
        summe.dazu(name.as_bytes());
        summe.dazu(&[0]);
        summe.dazu(&(inhalt.len() as u64).to_le_bytes());
        summe.dazu(&inhalt);
    }
    Ok(summe.fertig())
}

fn liste_lesen(wo: &Path) -> Result<BTreeMap<String, Eintrag>, String> {
    let datei = wo.join(LISTE);
    let mut liste = BTreeMap::new();
    let text = match fs::read_to_string(&datei) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(liste),
        Err(e) => return Err(format!("I kaun {} ned lesn: {e}", datei.display())),
    };
    for (nummer, zeile) in text.lines().enumerate() {
        let zeile = zeile.split('#').next().unwrap_or("").trim();
        if zeile.is_empty() {
            continue;
        }
        let gelesen = zeile.split_once('=').and_then(|(name, rest)| {
            let mut teile = rest.split_whitespace();
            let woher = teile.next()?;
            let summe = teile.next().and_then(|s| s.strip_prefix("sha256:")).unwrap_or("");
            Some((name.trim().to_string(), Eintrag { woher: woher.to_string(), summe: summe.to_string() }))
        });
        match gelesen {
            Some((name, eintrag)) if name_pruefen(&name).is_ok() => {
                liste.insert(name, eintrag);
            }
            _ => {
                return Err(format!(
                    "{}, Zeile {}: Des vasteh i ned. So schaut a Zeile aus: nom = wer/paket sha256:...",
                    datei.display(),
                    nummer + 1
                ));
            }
        }
    }
    Ok(liste)
}

fn liste_schreiben(wo: &Path, liste: &BTreeMap<String, Eintrag>) -> Result<(), String> {
    let mut text = String::from(
        "# De Pakete, de des Programm braucht. De Listn schreibt \"oida hol\"; \"oida hol\" ohne wos\n\
         # dahinta hoit ollas wieda (zum Beispü auf an aundan Rechna). De Prüfsumme sogt, ob no\n\
         # desöwe drin is wia beim letztn Moi.\n",
    );
    for (name, eintrag) in liste {
        text.push_str(&format!("{name} = {} sha256:{}\n", eintrag.woher, eintrag.summe));
    }
    let datei = wo.join(LISTE);
    fs::write(&datei, text).map_err(|e| format!("I kaun {} ned schreibn: {e}", datei.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adressen_lesen() {
        let github = |wer: &str, repo: &str, stand: Option<&str>| Quelle::Github {
            wer: wer.to_string(),
            repo: repo.to_string(),
            stand: stand.map(str::to_string),
        };
        assert_eq!(quelle_lesen("franz/wüafl").unwrap(), (github("franz", "wüafl", None), "wüafl".to_string()));
        assert_eq!(quelle_lesen("franz/wüafl@v1.2").unwrap().0, github("franz", "wüafl", Some("v1.2")));
        assert_eq!(quelle_lesen("https://github.com/franz/wuerfel.git").unwrap().0, github("franz", "wuerfel", None));
        assert_eq!(quelle_lesen("github.com/franz/wuerfel/").unwrap().1, "wuerfel");
        // Was kein "wer/paket" ist, wird nicht geraten; und nichts, was aus der Adresse hinausführt.
        for kaputt in ["wüafl", "a/b/c", "franz/", "/wüafl", "franz/..", "franz/wüafl@../x", "franz/wü afl", "franz/a?b=c"] {
            assert!(quelle_lesen(kaputt).is_err(), "{kaputt}");
        }
        assert!(quelle_lesen("./gibts/sicha/ned").unwrap_err().contains("find i ned"));
    }

    #[test]
    fn namen_von_paketen() {
        for gut in ["wüafl", "mei_paket", "a-b", "x2"] {
            assert!(name_pruefen(gut).is_ok(), "{gut}");
        }
        for schlecht in ["", ".vasteckt", "a/b", "a b", "..", "a\\b", "a.b"] {
            assert!(name_pruefen(schlecht).is_err(), "{schlecht}");
        }
    }

    #[test]
    fn liste_lesen_und_schreiben() {
        let ordner = std::env::temp_dir().join(format!("oida-paket-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&ordner);
        fs::create_dir_all(&ordner).unwrap();
        assert!(liste_lesen(&ordner).unwrap().is_empty());
        let mut liste = BTreeMap::new();
        liste.insert("wüafl".to_string(), Eintrag { woher: "franz/wüafl@v1".to_string(), summe: "abc".to_string() });
        liste.insert("kortn".to_string(), Eintrag { woher: "../kortn".to_string(), summe: "def".to_string() });
        liste_schreiben(&ordner, &liste).unwrap();
        assert_eq!(liste_lesen(&ordner).unwrap(), liste);
        let text = fs::read_to_string(ordner.join(LISTE)).unwrap();
        assert!(text.contains("kortn = ../kortn sha256:def\nwüafl = franz/wüafl@v1 sha256:abc\n"), "{text}");
        fs::write(ordner.join(LISTE), "wüafl franz/wüafl\n").unwrap();
        assert!(liste_lesen(&ordner).unwrap_err().contains("Zeile 1"));
        fs::remove_dir_all(&ordner).unwrap();
    }

    #[test]
    fn pruefsumme_haengt_am_inhalt_nicht_an_den_zeilenenden() {
        let ordner = std::env::temp_dir().join(format!("oida-paket-summe-{}", std::process::id()));
        let _ = fs::remove_dir_all(&ordner);
        fs::create_dir_all(ordner.join("a").join("tiaf")).unwrap();
        fs::create_dir_all(ordner.join("b").join("tiaf")).unwrap();
        fs::create_dir_all(ordner.join("b").join(".git")).unwrap();
        fs::write(ordner.join("a").join("x.oida"), "ans\nzwa\n").unwrap();
        fs::write(ordner.join("a").join("tiaf").join("y.txt"), "drei").unwrap();
        fs::write(ordner.join("b").join("x.oida"), "ans\r\nzwa\r\n").unwrap();
        fs::write(ordner.join("b").join("tiaf").join("y.txt"), "drei").unwrap();
        // Was nicht zum Paket gehört, zählt nicht mit.
        fs::write(ordner.join("b").join(".git").join("HEAD"), "wos aundas").unwrap();
        let (a, b) = (pruefsumme(&ordner.join("a")).unwrap(), pruefsumme(&ordner.join("b")).unwrap());
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        fs::write(ordner.join("b").join("tiaf").join("y.txt"), "vier").unwrap();
        assert_ne!(a, pruefsumme(&ordner.join("b")).unwrap());
        // Auch ein anderer Name ist ein anderer Inhalt.
        fs::write(ordner.join("b").join("tiaf").join("y.txt"), "drei").unwrap();
        fs::rename(ordner.join("b").join("tiaf").join("y.txt"), ordner.join("b").join("tiaf").join("z.txt")).unwrap();
        assert_ne!(a, pruefsumme(&ordner.join("b")).unwrap());
        fs::remove_dir_all(&ordner).unwrap();
    }
}
