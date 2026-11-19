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

const vorschlaege = {
    async provideCompletionItems(dokument, position) {
        const antwort = await untersuche(dokument);
        if (!antwort) {
            return [];
        }
        const davor = dokument.lineAt(position.line).text.slice(0, position.character).toUpperCase();
        return antwort.woerter.map((w) => {
            const eintrag = new vscode.CompletionItem(w.wort, vscode.CompletionItemKind.Keyword);
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
        });
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
        vscode.languages.registerCompletionItemProvider(sprache, vorschlaege),
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
