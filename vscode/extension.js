// Die VS-Code-Erweiterung für OIDA.
//
// Die Erweiterung kennt die Sprache nicht selbst. Für alles, was vom
// Wörterbuch abhängt, fragt sie den Compiler:  oida --editor  bekommt den
// Text aus dem Editor und antwortet mit Fehlern, Wortarten und Wörtern als
// JSON (siehe uebersetzer/src/editor.rs). So stimmt alles auch dann, wenn
// jemand die Wörter im Wörterbuch ändert.
'use strict';

const vscode = require('vscode');
const cp = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

// Die Wortarten. Die Reihenfolge muss zu `Art` in uebersetzer/src/editor.rs passen.
const ARTEN = ['keyword', 'type', 'function', 'variable', 'number', 'string', 'wahrheitswert', 'endwort', 'property'];
const LEGENDE = new vscode.SemanticTokensLegend(ARTEN, []);
const EXE = process.platform === 'win32' ? 'oida.exe' : 'oida';
// So lange wartet die Fehlerprüfung nach dem letzten Tastendruck (Millisekunden).
const WARTEZEIT = 200;

let ordnerDerErweiterung = '';
let fehlerListe;
let protokoll;
const tokensGeaendert = new vscode.EventEmitter();
/** Antworten vom Compiler je Dokument: uri -> { version, ergebnis: Promise } */
const antworten = new Map();
/** Laufende Wartezeiten je Dokument: uri -> Timer */
const wartend = new Map();
/** Meldungen, die schon gezeigt wurden (damit nicht bei jedem Tastendruck dieselbe kommt). */
const gezeigt = new Set();

// ------------------------------------------------------------ Compiler finden

function einstellung(name) {
    return (vscode.workspace.getConfiguration('oida').get(name) || '').trim();
}

function istDatei(pfad) {
    try {
        return fs.statSync(pfad).isFile();
    } catch {
        return false;
    }
}

/**
 * Sucht die oida.exe:
 * 1. Einstellung oida.compiler
 * 2. im geöffneten Ordner: target\release oder target\debug (die neuere)
 * 3. dort, wo sie beim Bauen der Erweiterung lag (compiler-ort.txt)
 * 4. im PATH
 */
function findeCompiler() {
    const eingestellt = einstellung('compiler');
    if (eingestellt) {
        return istDatei(eingestellt) ? eingestellt : null;
    }
    if (vscode.workspace.isTrusted) {
        let beste = null;
        for (const ordner of vscode.workspace.workspaceFolders || []) {
            for (const art of ['release', 'debug']) {
                const pfad = path.join(ordner.uri.fsPath, 'target', art, EXE);
                try {
                    const zeit = fs.statSync(pfad).mtimeMs;
                    if (!beste || zeit > beste.zeit) {
                        beste = { pfad, zeit };
                    }
                } catch {
                    // gibt es nicht
                }
            }
        }
        if (beste) {
            return beste.pfad;
        }
    }
    try {
        const gemerkt = fs.readFileSync(path.join(ordnerDerErweiterung, 'compiler-ort.txt'), 'utf8')
            .replace(/^﻿/, '').trim();
        if (gemerkt && istDatei(gemerkt)) {
            return gemerkt;
        }
    } catch {
        // gibt es nicht
    }
    for (const ordner of (process.env.PATH || '').split(path.delimiter)) {
        if (ordner && istDatei(path.join(ordner, EXE))) {
            return path.join(ordner, EXE);
        }
    }
    return null;
}

async function compilerFehlt(immerZeigen) {
    if (!immerZeigen && gezeigt.has('compiler')) {
        return;
    }
    gezeigt.add('compiler');
    const eingestellt = einstellung('compiler');
    const meldung = eingestellt
        ? `Heast! Bei oida.compiler steht ${eingestellt}, oba do is nix.`
        : 'Heast! I find den OIDA-Compiler (oida.exe) ned.';
    const knopf = 'oida.exe aussuchen';
    if (await vscode.window.showWarningMessage(meldung, knopf) === knopf) {
        await compilerAussuchen();
    }
}

async function compilerAussuchen() {
    const auswahl = await vscode.window.showOpenDialog({
        title: 'Wo liegt die oida.exe?',
        canSelectMany: false,
        filters: process.platform === 'win32' ? { Programm: ['exe'] } : undefined,
    });
    if (!auswahl || auswahl.length === 0) {
        return;
    }
    await vscode.workspace.getConfiguration('oida')
        .update('compiler', auswahl[0].fsPath, vscode.ConfigurationTarget.Global);
    gezeigt.delete('compiler');
    allesNeu();
}

// ------------------------------------------------------------ Compiler fragen

/** Zusätzliche Argumente für jeden Aufruf (eigenes Wörterbuch). */
function woerterbuchArgumente() {
    const woerterbuch = einstellung('woerterbuch');
    return woerterbuch ? ['--woerterbuch', woerterbuch] : [];
}

/**
 * Schickt den Text an  oida --editor  und gibt die Antwort zurück
 * (oder null, wenn es keinen Compiler gibt oder er nicht antwortet).
 */
function frageCompiler(text, arbeitsordner, datei) {
    const compiler = findeCompiler();
    if (!compiler) {
        compilerFehlt(false);
        return Promise.resolve(null);
    }
    return new Promise((fertig) => {
        let ausgabe = '';
        let meldungen = '';
        let prozess;
        try {
            // Die Datei sagt dem Compiler, von wo aus er sucht, was mit BRAUCH dazugeholt wird.
            const woher = datei ? [datei] : [];
            prozess = cp.spawn(compiler, ['--editor', ...woerterbuchArgumente(), ...woher], {
                cwd: arbeitsordner,
                windowsHide: true,
            });
        } catch (e) {
            melde(`I kaun ${compiler} ned startn: ${e.message}`);
            fertig(null);
            return;
        }
        const uhr = setTimeout(() => prozess.kill(), 10000);
        prozess.stdout.setEncoding('utf8').on('data', (stueck) => { ausgabe += stueck; });
        prozess.stderr.setEncoding('utf8').on('data', (stueck) => { meldungen += stueck; });
        prozess.on('error', (e) => {
            clearTimeout(uhr);
            melde(`I kaun ${compiler} ned startn: ${e.message}`);
            fertig(null);
        });
        prozess.on('close', (code) => {
            clearTimeout(uhr);
            if (code !== 0) {
                // Zum Beispiel ein Fehler im Wörterbuch.
                melde(meldungen.trim() || `Der Compiler hot mit Code ${code} aufghört.`);
                fertig(null);
                return;
            }
            try {
                fertig(JSON.parse(ausgabe));
            } catch (e) {
                melde(`Die Antwort vom Compiler versteh i ned: ${e.message}`);
                fertig(null);
            }
        });
        prozess.stdin.on('error', () => { /* Compiler schon weg; die Meldung kommt über close */ });
        prozess.stdin.end(text, 'utf8');
    });
}

/** Zeigt eine Meldung vom Compiler, jede nur einmal, und schreibt sie ins Protokoll. */
function melde(meldung) {
    protokoll.appendLine(meldung);
    if (!gezeigt.has(meldung)) {
        gezeigt.add(meldung);
        vscode.window.showWarningMessage(meldung.startsWith('Heast') ? meldung : `OIDA: ${meldung}`);
    }
}

/** Die Antwort vom Compiler für den aktuellen Stand eines Dokuments. */
function untersuche(dokument) {
    const schluessel = dokument.uri.toString();
    const alt = antworten.get(schluessel);
    if (alt && alt.version === dokument.version) {
        return alt.ergebnis;
    }
    const arbeitsordner = dokument.uri.scheme === 'file' ? path.dirname(dokument.uri.fsPath) : undefined;
    const datei = dokument.uri.scheme === 'file' ? dokument.fileName : undefined;
    const ergebnis = frageCompiler(dokument.getText(), arbeitsordner, datei);
    antworten.set(schluessel, { version: dokument.version, ergebnis });
    return ergebnis;
}

// --------------------------------------------------------------------- Fehler

async function zeigeFehler(dokument) {
    if (dokument.languageId !== 'oida') {
        return;
    }
    const version = dokument.version;
    const antwort = await untersuche(dokument);
    if (dokument.isClosed || dokument.version !== version) {
        return; // inzwischen weitergetippt
    }
    const fehler = ((antwort && antwort.fehler) || []).map((f) => {
        const bereich = new vscode.Range(f.zeile, f.spalte, f.zeile, f.spalte + f.laenge);
        const eintrag = new vscode.Diagnostic(bereich, f.meldung, vscode.DiagnosticSeverity.Error);
        eintrag.source = 'oida';
        return eintrag;
    });
    fehlerListe.set(dokument.uri, fehler);
}

function zeigeFehlerBald(dokument) {
    const schluessel = dokument.uri.toString();
    clearTimeout(wartend.get(schluessel));
    wartend.set(schluessel, setTimeout(() => {
        wartend.delete(schluessel);
        zeigeFehler(dokument);
    }, WARTEZEIT));
}

/** Nach einer Änderung am Wörterbuch oder an den Einstellungen: alles neu fragen. */
function allesNeu() {
    antworten.clear();
    gezeigt.clear();
    tokensGeaendert.fire();
    for (const dokument of vscode.workspace.textDocuments) {
        zeigeFehler(dokument);
    }
}

// ------------------------------------------------------------------ Einfärben

const einfaerber = {
    onDidChangeSemanticTokens: tokensGeaendert.event,
    async provideDocumentSemanticTokens(dokument) {
        const antwort = await untersuche(dokument);
        const bau = new vscode.SemanticTokensBuilder(LEGENDE);
        for (const [zeile, spalte, laenge, art] of (antwort && antwort.tokens) || []) {
            bau.push(zeile, spalte, laenge, art, 0);
        }
        return bau.build();
    },
};

// ----------------------------------------------------------------- Vorschläge

/**
 * Sollen die Vorschläge an dieser Stelle klein eingesetzt werden? Der Compiler nimmt beides
 * (WENN und wenn). Die Einstellung `oida.schreibweise` legt es fest; bei "wie getippt"
 * entscheidet, was gerade getippt wurde, und wenn dort noch kein Buchstabe steht, wie die
 * Wörter der Sprache in der Datei bisher geschrieben sind.
 */
function kleinSchreiben(dokument, getippt, antwort) {
    const wahl = einstellung('schreibweise');
    if (wahl === 'klein') {
        return true;
    }
    if (wahl === 'GROSS') {
        return false;
    }
    const hatKlein = /\p{Ll}/u.test(getippt);
    const hatGross = /\p{Lu}/u.test(getippt);
    if (hatKlein !== hatGross) {
        return hatKlein;
    }
    let klein = 0;
    let gross = 0;
    for (const [zeile, spalte, laenge, art] of antwort.tokens || []) {
        if (ARTEN[art] !== 'keyword' && ARTEN[art] !== 'endwort') {
            continue;
        }
        const wort = dokument.lineAt(zeile).text.substr(spalte, laenge);
        if (/\p{Ll}/u.test(wort) && !/\p{Lu}/u.test(wort)) {
            klein++;
        } else if (/\p{Lu}/u.test(wort) && !/\p{Ll}/u.test(wort)) {
            gross++;
        }
    }
    return klein > gross;
}

/** Die Bausteine (wenn, solong, funktiaun ...) aus snippets/oida.json; die Datei erzeugt der Compiler. */
let bausteine = null;

function ladeBausteine() {
    if (bausteine === null) {
        bausteine = [];
        try {
            const datei = path.join(ordnerDerErweiterung, 'snippets', 'oida.json');
            for (const [name, b] of Object.entries(JSON.parse(fs.readFileSync(datei, 'utf8')))) {
                if (b && typeof b === 'object' && b.prefix && b.body) {
                    bausteine.push({ name, kuerzel: String(b.prefix), text: [].concat(b.body).join('\n'), was: b.description || '' });
                }
            }
        } catch (fehler) {
            protokoll.appendLine(`Die Bausteine lassen sich nicht lesen: ${fehler.message}`);
        }
    }
    return bausteine;
}

/** Ein Baustein in Kleinbuchstaben: Nur die Wörter der Sprache (ganz groß geschrieben) werden klein, `${1:Name}` bleibt. */
function kleinerBaustein(text) {
    return text.replace(/(?<![\p{L}\p{N}_])\p{Lu}[\p{Lu}_]+(?![\p{L}\p{N}_])/gu, (wort) => wort.toLowerCase());
}

// Die Namen aus dem Programm (siehe `alle_namen` in uebersetzer/src/editor.rs) und ihr Zeichen in der Liste.
const NAMENS_ARTEN = {
    funktion: vscode.CompletionItemKind.Function,
    methode: vscode.CompletionItemKind.Method,
    typ: vscode.CompletionItemKind.Class,
    wert: vscode.CompletionItemKind.EnumMember,
    feld: vscode.CompletionItemKind.Field,
    variable: vscode.CompletionItemKind.Variable,
    fix: vscode.CompletionItemKind.Constant,
};

/** Ein Name aus dem Programm als Vorschlag. `klammerDa`: Hinter dem Cursor steht schon eine Klammer. */
function nameAlsVorschlag(n, klammerDa) {
    const woher = n.zu || n.aus;
    const eintrag = new vscode.CompletionItem(
        woher ? { label: n.name, description: woher } : n.name, NAMENS_ARTEN[n.art] || vscode.CompletionItemKind.Text);
    eintrag.detail = n.text;
    if ((n.art === 'funktion' || n.art === 'methode') && !klammerDa) {
        // Mit Klammern und den Namen der Parameter zum Überschreiben; TAB springt zum nächsten.
        const parameter = (n.parameter || []).map((p, i) => `\${${i + 1}:${p}}`).join(', ');
        eintrag.insertText = new vscode.SnippetString(`${n.name}(${parameter})`);
    }
    const hilfe = new vscode.MarkdownString();
    hilfe.appendCodeblock(n.text, 'oida');
    if (n.doku) {
        hilfe.appendText(n.doku + '\n');
    }
    if (n.aus) {
        hilfe.appendMarkdown(`\n*aus ${n.aus}*`);
    }
    eintrag.documentation = hilfe;
    return eintrag;
}

/** Jeder Name nur einmal; kommt er in mehreren Klassen vor, stehen alle dabei. */
function ohneDoppelte(namen) {
    const nachName = new Map();
    for (const n of namen) {
        const schluessel = `${n.art}:${n.name}`;
        const schon = nachName.get(schluessel);
        if (!schon) {
            nachName.set(schluessel, { ...n });
        } else if (n.zu && !schon.zu.split(', ').includes(n.zu)) {
            schon.zu += `, ${n.zu}`;
        }
    }
    return [...nachName.values()];
}

// ------------------------------------------- Was für ein Ding steht vor dem Punkt?
//
// Der Compiler meldet zu Feldern, Parametern und Funktionen den Typ, der dasteht, in kurzer
// Schreibweise (siehe `typ_lesen` in uebersetzer/src/editor.rs): Punkt, ?T (VIELLEICHT T),
// [T] (LISTN VON T), {K:V} (VAZEICHNIS), INT, FLOAT, BOOLEAN, TEXT, ERROR, FN. Zu Variablen
// ohne Typ meldet er, womit sie anfangen (eine "Kette" wie ["spü", "gegna", "[]"]). Daraus
// wird hier ausgerechnet, was vor einem Punkt steht.

/** Zum Nachschlagen: Typen, Felder, Methoden und Funktionen nach Namen. */
function wissenAus(namen) {
    const w = { namen, typen: new Map(), felder: new Map(), methoden: new Map(), funktionen: new Map(), raeume: new Set() };
    for (const n of namen) {
        if (n.raum) {
            w.raeume.add(n.raum);
        }
        if (n.art === 'typ') {
            w.typen.set(n.name, n);
        } else if (n.art === 'feld') {
            w.felder.set(`${n.zu}.${n.name}`, n);
        } else if (n.art === 'methode') {
            w.methoden.set(`${n.zu}.${n.name}`, n);
        } else if (n.art === 'funktion') {
            // Eine Funktion aus einem Namensraum (BRAUCH ... OIS w) steht unter "w.name", die anderen unter ".name".
            w.funktionen.set(`${n.raum || ''}.${n.name}`, n);
        }
    }
    return w;
}

/** Eine Klasse und alle, von denen sie erbt (sie selbst zuerst). */
function ahnen(w, name) {
    const aus = [];
    let jetzt = name;
    while (jetzt && w.typen.has(jetzt) && !aus.includes(jetzt)) {
        aus.push(jetzt);
        jetzt = w.typen.get(jetzt).eltern;
    }
    return aus;
}

/** VIELLEICHT T zählt für die Vorschläge wie T. */
function ohneVielleicht(typ) {
    return typ.replace(/^\?+/, '');
}

/** Was in einer Liste oder einem Verzeichnis steckt: aus [T] wird T, aus {K:V} wird V. */
function elementVon(typ) {
    const t = ohneVielleicht(typ);
    if (t.startsWith('[') && t.endsWith(']')) {
        return t.slice(1, -1) || null;
    }
    if (t.startsWith('{') && t.endsWith('}') && t.includes(':')) {
        return t.slice(t.indexOf(':') + 1, -1) || null;
    }
    return null;
}

/** Die Funktion oder Methode dieser Datei, in der die Zeile liegt. */
function funktionUm(w, zeile) {
    return w.namen.find((n) => (n.art === 'funktion' || n.art === 'methode') && n.aus === '' && n.von <= zeile && zeile <= n.bis);
}

/**
 * Gilt die Variable in dieser Zeile? Eine aus einer Funktion nur dort, eine aus dem
 * Hauptprogramm nur außerhalb der Funktionen (die sehen sie nicht) und erst, wenn sie angelegt ist.
 */
function giltHier(n, zeile, drin) {
    return n.innen ? n.von <= zeile && zeile <= n.bis : !drin && n.von <= zeile;
}

/** Die Variable mit diesem Namen, die in der Zeile gilt (die zuletzt angelegte). */
function variableHier(w, name, zeile, ohne) {
    const drin = funktionUm(w, zeile);
    let beste;
    for (const n of w.namen) {
        if (n.art === 'variable' && n.name === name && n !== ohne && giltHier(n, zeile, drin) && (!beste || n.von >= beste.von)) {
            beste = n;
        }
    }
    return beste;
}

function typDerVariable(w, v, tiefe) {
    if (v.typ) {
        return v.typ;
    }
    const typ = typVonKette(w, v.kette, v.von, tiefe + 1, v);
    return v.jedes ? typ && elementVon(typ) : typ;
}

/** Der Typ einer Kette wie ["spü", "gegna", "[]", "nam"] in dieser Zeile, oder null, wenn er sich nicht ausrechnen lässt. */
function typVonKette(w, kette, zeile, tiefe = 0, ohne = undefined) {
    if (!kette || kette.length === 0 || tiefe > 12) {
        return null;
    }
    let typ = null;
    // w.mehrmois(3): Der Name des Raums zählt nicht als Schritt, er sagt nur, wo die Funktion steht.
    let raum = '';
    if (kette.length >= 2 && w.raeume.has(kette[0]) && !variableHier(w, kette[0], zeile, ohne)) {
        raum = kette[0];
        kette = kette.slice(1);
    }
    const erstes = kette[0];
    if (erstes === '@') {
        // MEI: die Klasse der Methode, in der wir stehen.
        const drin = funktionUm(w, zeile);
        typ = drin && drin.art === 'methode' ? drin.zu : null;
    } else if (erstes.endsWith('()')) {
        const name = erstes.slice(0, -2);
        if (w.typen.has(name)) {
            typ = name; // Punkt(1, 2) baut einen Punkt
        } else if (w.funktionen.has(`${raum}.${name}`)) {
            typ = w.funktionen.get(`${raum}.${name}`).typ || null;
        }
    } else {
        const v = variableHier(w, erstes, zeile, ohne);
        typ = v ? typDerVariable(w, v, tiefe) : null;
    }
    for (const schritt of kette.slice(1)) {
        if (!typ || schritt === '?') {
            return null;
        }
        if (schritt === '[]') {
            typ = elementVon(typ);
            continue;
        }
        const ruf = schritt.endsWith('()');
        const name = ruf ? schritt.slice(0, -2) : schritt;
        let gefunden;
        for (const klasse of ahnen(w, ohneVielleicht(typ))) {
            gefunden = (ruf ? w.methoden : w.felder).get(`${klasse}.${name}`);
            if (gefunden) {
                break;
            }
        }
        typ = gefunden ? gefunden.typ || null : null;
    }
    return typ;
}

/**
 * Liest den Text vor einem Punkt von hinten als Kette: "  spü.gegna[i].such(1)" wird zu
 * ["spü", "gegna", "[]", "such()"]. `selbst` sind die Wörter für MEI (groß geschrieben).
 * Gibt null zurück, wenn dort etwas anderes steht (eine Klammer, eine Zahl).
 */
function ketteVorDemPunkt(text, selbst) {
    const kette = [];
    let i = text.length;
    for (;;) {
        while (i > 0 && /\s/.test(text[i - 1])) {
            i--;
        }
        // Von hinten: zuerst die Klammern eines Aufrufs oder einer Stelle, dann der Name.
        const hinten = [];
        while (i > 0 && (text[i - 1] === ')' || text[i - 1] === ']')) {
            const zu = text[i - 1];
            const auf = zu === ')' ? '(' : '[';
            let tiefe = 0;
            let j = i;
            while (j > 0) {
                j--;
                if (text[j] === ')' || text[j] === ']') {
                    tiefe++;
                } else if (text[j] === '(' || text[j] === '[') {
                    tiefe--;
                    if (tiefe === 0) {
                        break;
                    }
                }
            }
            if (tiefe !== 0 || text[j] !== auf) {
                return null;
            }
            hinten.unshift(zu === ')' ? '()' : '[]');
            i = j;
            while (i > 0 && /\s/.test(text[i - 1])) {
                i--;
            }
        }
        const name = /[\p{L}\p{N}_]+$/u.exec(text.slice(0, i));
        if (!name || /^\p{N}/u.test(name[0])) {
            return null;
        }
        i -= name[0].length;
        const teile = [selbst.includes(name[0].toUpperCase()) ? '@' : name[0]];
        for (const h of hinten) {
            if (h === '[]') {
                teile.push('[]');
            } else if (teile.length === 1 && teile[0] !== '@') {
                teile[0] += '()';
            } else {
                teile.push('?');
            }
        }
        kette.unshift(...teile);
        while (i > 0 && /\s/.test(text[i - 1])) {
            i--;
        }
        if (i > 0 && text[i - 1] === '.') {
            i--;
            continue;
        }
        return kette;
    }
}

/** Was hinter dem Punkt vorgeschlagen wird. `vor` ist der Text der Zeile bis vor den Punkt. */
function gliederHinterPunkt(antwort, vor, zeile) {
    const namen = antwort.namen || [];
    const selbst = (antwort.woerter || []).filter((x) => x.fest === 'THIS').map((x) => x.wort.toUpperCase());
    const kette = ketteVorDemPunkt(vor, selbst);
    if (!kette) {
        return [];
    }
    const w = wissenAus(namen);
    const istRaum = w.raeume.has(kette[0]) && !variableHier(w, kette[0], zeile);
    // Hinter dem Namen eines Raums (BRAUCH ... OIS w): was es dort gibt.
    if (kette.length === 1 && istRaum) {
        return namen.filter((n) => n.raum === kette[0] && ['funktion', 'typ', 'fix'].includes(n.art));
    }
    // Hinter dem Namen einer Aufzählung: ihre Werte (Ampl.ROT, w.Ampl.ROT).
    if (kette.length === 2 && istRaum) {
        const werte = namen.filter((n) => n.art === 'wert' && n.zu === kette[1] && n.raum === kette[0]);
        if (werte.length > 0) {
            return werte;
        }
    }
    if (kette.length === 1 && !variableHier(w, kette[0], zeile)) {
        const werte = namen.filter((n) => n.art === 'wert' && n.zu === kette[0] && !n.raum);
        if (werte.length > 0) {
            return werte;
        }
    }
    // Was GHEIM ist, gibt es nur in den Methoden der Klasse selbst.
    const drin = funktionUm(w, zeile);
    const hier = drin && drin.art === 'methode' ? drin.zu : '';
    const alle = namen.filter((n) => (n.art === 'feld' || n.art === 'methode') && (!n.gheim || n.zu === hier));
    const typ = typVonKette(w, kette, zeile);
    if (!typ) {
        // Nicht auszurechnen (zum Beispiel ein Wert aus einer Vorlage): dann lieber alles als nichts.
        return ohneDoppelte(alle);
    }
    const klasse = ohneVielleicht(typ);
    if (klasse === 'ERROR') {
        return [{ name: 'meldung', art: 'feld', text: 'meldung KETTN', doku: 'Was schiefgegangen ist.', aus: '', zu: 'FEHLA', parameter: [] }];
    }
    // Die Felder und Methoden der Klasse und derer, von denen sie erbt; die eigene zuerst
    // (so gewinnt eine Methode, die eine geerbte ersetzt).
    const kreis = ahnen(w, klasse);
    const gefunden = [];
    for (const k of kreis) {
        for (const n of alle) {
            if (n.zu === k && !gefunden.some((g) => g.art === n.art && g.name === n.name)) {
                gefunden.push(n);
            }
        }
    }
    return gefunden;
}

const vorschlaege = {
    async provideCompletionItems(dokument, position) {
        const antwort = await untersuche(dokument);
        if (!antwort) {
            return [];
        }
        const ganzeZeile = dokument.lineAt(position.line).text;
        const zeile = ganzeZeile.slice(0, position.character);
        const namen = antwort.namen || [];
        const klammerDa = ganzeZeile.slice(position.character).startsWith('(');
        // Hinter einem Punkt gibt es nur, was zu dem gehört, was davor steht: hinter dem Namen
        // einer Aufzählung ihre Werte (Ampl.ROT), sonst die Felder und Methoden dieses Dings.
        const punkt = /\.\s*[\p{L}\p{N}_]*$/u.exec(zeile);
        if (punkt) {
            return gliederHinterPunkt(antwort, zeile.slice(0, punkt.index), position.line).map((n) => nameAlsVorschlag(n, klammerDa));
        }
        // Variablen nur dort, wo sie gelten.
        const drin = funktionUm({ namen }, position.line);
        const hier = namen.filter((n) =>
            (['funktion', 'typ', 'fix'].includes(n.art) && !n.raum) || (n.art === 'variable' && giltHier(n, position.line, drin)));
        const namensListe = ohneDoppelte(hier).map((n) => nameAlsVorschlag(n, klammerDa));
        for (const raum of new Set(namen.map((n) => n.raum).filter(Boolean))) {
            const eintrag = new vscode.CompletionItem({ label: raum, description: 'Namensraum' }, vscode.CompletionItemKind.Module);
            eintrag.detail = `${raum}.name: was aus der Datei kommt, die mit OIS ${raum} dazugeholt ist`;
            namensListe.push(eintrag);
        }
        const davor = zeile.toUpperCase();
        const klein = kleinSchreiben(dokument, /[\p{L}\p{N}_]*$/u.exec(zeile)[0], antwort);
        const schreib = (text) => (klein ? text.toLowerCase() : text);
        const bausteinListe = ladeBausteine().map((b) => {
            const eintrag = new vscode.CompletionItem({ label: schreib(b.name), description: 'Baustein' }, vscode.CompletionItemKind.Snippet);
            const text = klein ? kleinerBaustein(b.text) : b.text;
            eintrag.insertText = new vscode.SnippetString(text);
            eintrag.filterText = b.kuerzel;
            eintrag.detail = `OIDA-Baustein: ${b.was}`;
            eintrag.documentation = new vscode.MarkdownString().appendCodeblock(text.replace(/\$\{\d+:([^}]*)\}/g, '$1').replace(/\$\d+/g, ''), 'oida');
            return eintrag;
        });
        return antwort.woerter.map((w) => {
            const eintrag = new vscode.CompletionItem(schreib(w.wort), vscode.CompletionItemKind.Keyword);
            eintrag.detail = w.fest === w.wort ? 'OIDA' : `OIDA (${w.fest})`;
            // "SOG A|" soll zu "SOG AU" werden und nicht zu "SOG SOG AU":
            // steht vor dem Cursor schon der Anfang der Wortfolge, wird der ersetzt.
            const gross = w.wort.toUpperCase();
            for (let n = gross.length - 1; n > gross.indexOf(' ') && gross.includes(' '); n--) {
                const anfang = gross.slice(0, n);
                if (davor.endsWith(anfang) && !/[\p{L}\p{N}_]$/u.test(davor.slice(0, davor.length - n))) {
                    eintrag.range = new vscode.Range(position.translate(0, -n), position);
                    eintrag.filterText = dokument.getText(eintrag.range) + w.wort.slice(n);
                    break;
                }
            }
            return eintrag;
        }).concat(bausteinListe, namensListe);
    },
};

// ----------------------------------------------------- Übersetzen und Starten

async function ausfuehren(nurUebersetzen) {
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'oida') {
        vscode.window.showInformationMessage('Moch zerst a .oida-Datei auf.');
        return undefined;
    }
    const dokument = editor.document;
    if (dokument.isUntitled || dokument.uri.scheme !== 'file') {
        vscode.window.showInformationMessage('Speicher de Datei zerst o (mit .oida am End).');
        return undefined;
    }
    if (dokument.isDirty && !(await dokument.save())) {
        return undefined;
    }
    const compiler = findeCompiler();
    if (!compiler) {
        await compilerFehlt(true);
        return undefined;
    }

    const datei = dokument.uri.fsPath;
    const argumente = [datei, ...woerterbuchArgumente()];
    if (!nurUebersetzen) {
        // Beim Starten kommt das Programm in den Temp-Ordner, damit der eigene Ordner sauber bleibt.
        const ordner = path.join(os.tmpdir(), 'oida-vscode');
        fs.mkdirSync(ordner, { recursive: true });
        argumente.push('-o', path.join(ordner, path.basename(datei, path.extname(datei)) + (process.platform === 'win32' ? '.exe' : '')), '--renn');
    }
    const aufgabe = new vscode.Task(
        { type: 'oida' },
        vscode.TaskScope.Workspace,
        nurUebersetzen ? 'Übersetzen' : path.basename(datei),
        'OIDA',
        new vscode.ProcessExecution(compiler, argumente, { cwd: path.dirname(datei) }),
        [],
    );
    aufgabe.presentationOptions = {
        reveal: vscode.TaskRevealKind.Always,
        panel: vscode.TaskPanelKind.Shared,
        clear: true,
        showReuseMessage: false,
        focus: false,
    };
    return vscode.tasks.executeTask(aufgabe);
}

async function woerterbuchOeffnen() {
    const editor = vscode.window.activeTextEditor;
    const antwort = editor && editor.document.languageId === 'oida'
        ? await untersuche(editor.document)
        : await frageCompiler('', undefined);
    if (!antwort) {
        return;
    }
    if (istDatei(antwort.woerterbuch)) {
        await vscode.window.showTextDocument(vscode.Uri.file(antwort.woerterbuch));
    } else {
        vscode.window.showInformationMessage(
            'Es gilt des Wörterbuch, des in da oida.exe eingebaut is. '
            + 'Leg a woerterbuch.txt neben de oida.exe, dann kaunst as ändern.',
        );
    }
}

// --------------------------------------------------------------------- Anfang

function activate(kontext) {
    ordnerDerErweiterung = kontext.extensionPath;
    fehlerListe = vscode.languages.createDiagnosticCollection('oida');
    protokoll = vscode.window.createOutputChannel('OIDA');
    const sprache = { language: 'oida' };

    kontext.subscriptions.push(
        fehlerListe,
        protokoll,
        tokensGeaendert,
        vscode.languages.registerDocumentSemanticTokensProvider(sprache, einfaerber, LEGENDE),
        // Der Punkt löst die Vorschläge auch aus: punkt.| und Ampl.|
        vscode.languages.registerCompletionItemProvider(sprache, vorschlaege, '.'),
        vscode.commands.registerCommand('oida.starten', () => ausfuehren(false)),
        vscode.commands.registerCommand('oida.uebersetzen', () => ausfuehren(true)),
        vscode.commands.registerCommand('oida.compilerAussuchen', compilerAussuchen),
        vscode.commands.registerCommand('oida.woerterbuchOeffnen', woerterbuchOeffnen),
        vscode.workspace.onDidOpenTextDocument(zeigeFehler),
        vscode.workspace.onDidChangeTextDocument((e) => {
            if (e.document.languageId === 'oida') {
                zeigeFehlerBald(e.document);
            }
        }),
        vscode.workspace.onDidCloseTextDocument((dokument) => {
            const schluessel = dokument.uri.toString();
            clearTimeout(wartend.get(schluessel));
            wartend.delete(schluessel);
            antworten.delete(schluessel);
            fehlerListe.delete(dokument.uri);
        }),
        // Wörterbuch gespeichert: alles neu einfärben und prüfen.
        vscode.workspace.onDidSaveTextDocument((dokument) => {
            const name = path.basename(dokument.uri.fsPath).toLowerCase();
            const eigenes = einstellung('woerterbuch');
            if (name === 'woerterbuch.txt' || (eigenes && path.resolve(eigenes) === dokument.uri.fsPath)) {
                allesNeu();
            }
        }),
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('oida')) {
                allesNeu();
            }
        }),
        vscode.workspace.onDidGrantWorkspaceTrust(allesNeu),
    );

    for (const dokument of vscode.workspace.textDocuments) {
        zeigeFehler(dokument);
    }
}

function deactivate() {
    for (const uhr of wartend.values()) {
        clearTimeout(uhr);
    }
}

module.exports = { activate, deactivate };
