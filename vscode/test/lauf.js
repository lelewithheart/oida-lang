// Test der Erweiterung in einem echten VS Code.
//
// VS Code lädt diese Datei, wenn man es mit --extensionTestsPath startet
// (das macht testen.ps1) und ruft run() auf. Schlägt etwas fehl, hört
// VS Code mit einem Fehlercode auf. Der Bericht landet in der Datei aus
// der Umgebungsvariable OIDA_TESTBERICHT.
//
// Geöffnet sein muss der Projektordner (dort liegen beispiele/ und target/).
'use strict';

const vscode = require('vscode');
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');

const bericht = [];
const wurzel = vscode.workspace.workspaceFolders[0].uri.fsPath;
const arbeit = fs.mkdtempSync(path.join(os.tmpdir(), 'oida-vscode-test-'));

function warte(ms) {
    return new Promise((fertig) => setTimeout(fertig, ms));
}

/** Fragt so lange nach, bis `pruefung` etwas Wahres liefert (höchstens `ms`). */
async function bis(wofuer, pruefung, ms = 15000) {
    const ende = Date.now() + ms;
    for (;;) {
        const ergebnis = await pruefung();
        if (ergebnis) {
            return ergebnis;
        }
        if (Date.now() > ende) {
            throw new Error(`Zeit abgelaufen: ${wofuer}`);
        }
        await warte(100);
    }
}

async function oeffne(datei) {
    const dokument = await vscode.workspace.openTextDocument(datei);
    await vscode.window.showTextDocument(dokument);
    return dokument;
}

function schreibe(name, inhalt) {
    const datei = path.join(arbeit, name);
    fs.writeFileSync(datei, inhalt, 'utf8');
    return datei;
}

/** Die eingefärbten Wörter eines Dokuments als Liste von [zeile, spalte, laenge, art]. */
async function tokens(dokument) {
    const legende = await vscode.commands.executeCommand(
        'vscode.provideDocumentSemanticTokensLegend', dokument.uri);
    const antwort = await vscode.commands.executeCommand(
        'vscode.provideDocumentSemanticTokens', dokument.uri);
    assert.ok(legende && antwort, 'keine Antwort vom Einfärber');
    const liste = [];
    let zeile = 0;
    let spalte = 0;
    for (let i = 0; i < antwort.data.length; i += 5) {
        zeile += antwort.data[i];
        spalte = antwort.data[i] === 0 ? spalte + antwort.data[i + 1] : antwort.data[i + 1];
        liste.push([zeile, spalte, antwort.data[i + 2], legende.tokenTypes[antwort.data[i + 3]]]);
    }
    return liste;
}

/** Führt einen Befehl aus, der eine Aufgabe startet, und wartet auf deren Ende. */
async function laufenLassen(befehl) {
    const ende = new Promise((fertig) => {
        const abo = vscode.tasks.onDidEndTaskProcess((e) => {
            if (e.execution.task.source === 'OIDA') {
                abo.dispose();
                fertig(e.exitCode);
            }
        });
    });
    const gestartet = await vscode.commands.executeCommand(befehl);
    assert.ok(gestartet, `${befehl} hat nichts gestartet`);
    return Promise.race([
        ende,
        warte(90000).then(() => { throw new Error(`Zeit abgelaufen: ${befehl}`); }),
    ]);
}

const pruefungen = {
    async 'Erweiterung wird geladen und .oida erkannt'() {
        const dokument = await oeffne(path.join(wurzel, 'beispiele', 'fizzbuzz.oida'));
        assert.strictEqual(dokument.languageId, 'oida');
        const erweiterung = vscode.extensions.getExtension('lelewithheart.oida');
        assert.ok(erweiterung, 'Erweiterung lelewithheart.oida ist nicht da');
        await bis('Erweiterung aktiv', () => erweiterung.isActive);
    },

    async 'Programm ohne Fehler hat keine Fehler'() {
        const dokument = await oeffne(path.join(wurzel, 'beispiele', 'noten.oida'));
        const liste = await tokens(dokument); // erst wenn der Compiler geantwortet hat
        assert.ok(liste.length > 50, `nur ${liste.length} Wörter eingefärbt`);
        await warte(500);
        assert.deepStrictEqual(vscode.languages.getDiagnostics(dokument.uri), []);
    },

    async 'Klassen, Methoden und Felder werden eingefärbt'() {
        const dokument = await oeffne(schreibe('klasse.oida',
            'KLASSE Wesen\n    lebn ZOI = 3\n    FUNKTIAUN sog()\n        SCHREIB(SÖWA.lebn) OIDA\n    PASST SCHO\n'
            + 'PASST SCHO\nSOG AU w = Wesen() OIDA\nw.sog() OIDA\n'));
        const liste = await tokens(dokument);
        const art = (zeile, spalte) => (liste.find((t) => t[0] === zeile && t[1] === spalte) || [])[3];
        assert.strictEqual(art(0, 0), 'keyword');    // KLASSE
        assert.strictEqual(art(0, 7), 'type');       // Wesen, wo die Klasse steht
        assert.strictEqual(art(6, 11), 'type');      // Wesen() beim Anlegen
        assert.strictEqual(art(3, 16), 'keyword');   // SÖWA
        assert.strictEqual(art(3, 21), 'property');  // .lebn
        assert.strictEqual(art(7, 0), 'variable');   // w
        assert.strictEqual(art(7, 2), 'function');   // .sog()
        await warte(500);
        assert.deepStrictEqual(vscode.languages.getDiagnostics(dokument.uri), []);
    },

    async 'BRAUCH holt andere Dateien und die Bibliothek dazu'() {
        schreibe('teil.oida', 'KLASSE Wesen\n    name KETTN\nPASST SCHO\n');
        const dokument = await oeffne(schreibe('haupt.oida',
            'BRAUCH "teil.oida" OIDA\nBRAUCH listen OIDA\nSOG AU w = Wesen("Hansi") OIDA\n'
            + 'SCHREIB(summe([1, 2]), w.name) OIDA\n'));
        const liste = await tokens(dokument);
        const art = (zeile, spalte) => (liste.find((t) => t[0] === zeile && t[1] === spalte) || [])[3];
        assert.strictEqual(art(0, 0), 'keyword');   // BRAUCH
        assert.strictEqual(art(2, 11), 'type');     // Wesen aus der anderen Datei
        await warte(500);
        assert.deepStrictEqual(vscode.languages.getDiagnostics(dokument.uri), []);
    },

    async 'Ein Fehler in einer dazugeholten Datei steht beim BRAUCH'() {
        schreibe('kaputt.oida', 'FUNKTIAUN hoib(x ZOI) GIBT ZRUCK ZOI\n    GIB ZRUCK x + "a" OIDA\nPASST SCHO\n');
        const dokument = await oeffne(schreibe('haupt2.oida', '# oben\nBRAUCH "kaputt.oida" OIDA\nSCHREIB(hoib(2)) OIDA\n'));
        const fehler = await bis('Fehler erscheint', () => {
            const liste = vscode.languages.getDiagnostics(dokument.uri);
            return liste.length > 0 ? liste : null;
        });
        assert.strictEqual(fehler.length, 1);
        assert.ok(/kaputt\.oida, Zeile 2: A ZOI \+ a KETTN\?/.test(fehler[0].message), fehler[0].message);
        assert.strictEqual(fehler[0].range.start.line, 1);
    },

    async 'Fehlendes OIDA wird unterstrichen'() {
        const dokument = await oeffne(schreibe('fehler.oida', 'SOG AU alter = 17\nSCHREIB(alter) OIDA\n'));
        const fehler = await bis('Fehler erscheint', () => {
            const liste = vscode.languages.getDiagnostics(dokument.uri);
            return liste.length > 0 ? liste : null;
        });
        assert.strictEqual(fehler.length, 1);
        assert.strictEqual(fehler[0].message, 'Heast, wo is des OIDA?');
        assert.strictEqual(fehler[0].severity, vscode.DiagnosticSeverity.Error);
        const b = fehler[0].range;
        assert.deepStrictEqual([b.start.line, b.start.character, b.end.line, b.end.character], [0, 15, 0, 17]);
    },

    async 'Nach dem Ausbessern verschwindet der Fehler'() {
        const dokument = vscode.window.activeTextEditor.document;
        assert.ok(dokument.fileName.endsWith('fehler.oida'));
        const aenderung = new vscode.WorkspaceEdit();
        aenderung.insert(dokument.uri, new vscode.Position(0, 17), ' OIDA');
        assert.ok(await vscode.workspace.applyEdit(aenderung));
        await bis('Fehler verschwindet', () => vscode.languages.getDiagnostics(dokument.uri).length === 0);
    },

    async 'Mehrere Typfehler auf einmal'() {
        const dokument = await oeffne(schreibe('typen.oida',
            'SOG AU alter = 17 OIDA\nalter = "siebzehn" OIDA\nSCHREIB(wos) OIDA\n'));
        const fehler = await bis('zwei Fehler', () => {
            const liste = vscode.languages.getDiagnostics(dokument.uri);
            return liste.length === 2 ? liste : null;
        });
        assert.deepStrictEqual(fehler.map((f) => f.message).sort(), [
            "'alter' is a ZOI, do kaunst ka KETTN einetuan.",
            "'wos' kenn i ned. Host des mit SOG AU augsogt?",
        ]);
    },

    async 'Wörter werden nach ihrer Art eingefärbt'() {
        const dokument = await oeffne(schreibe('farben.oida',
            'SOG AU größe = 5 OIDA\nSCHREIB(hoch(größe), "ä", WOAHR, ZOI(1.5)) OIDA\n'));
        assert.deepStrictEqual(await tokens(dokument), [
            [0, 0, 6, 'keyword'], [0, 7, 5, 'variable'], [0, 15, 1, 'number'], [0, 17, 4, 'endwort'],
            [1, 0, 7, 'keyword'], [1, 8, 4, 'function'], [1, 13, 5, 'variable'], [1, 21, 3, 'string'],
            [1, 26, 5, 'wahrheitswert'], [1, 33, 3, 'type'], [1, 37, 3, 'number'], [1, 43, 4, 'endwort'],
        ]);
    },

    async 'Vorschläge kommen aus dem Wörterbuch'() {
        const dokument = await oeffne(schreibe('vorschlag.oida', 'SO\nSOG A\n'));
        const hole = async (zeile, spalte) => {
            const liste = await vscode.commands.executeCommand(
                'vscode.executeCompletionItemProvider', dokument.uri, new vscode.Position(zeile, spalte));
            return liste.items.filter((e) => (e.detail || '').startsWith('OIDA'));
        };
        const erste = (await hole(0, 2)).map((e) => e.label);
        for (const wort of ['SOG AU', 'SOLONG', 'PASST SCHO', 'KETTN', 'OIDA']) {
            assert.ok(erste.includes(wort), `${wort} fehlt in den Vorschlägen: ${erste.join(', ')}`);
        }
        // "SOG A|" -> "SOG AU": der schon getippte Anfang wird ersetzt.
        const sogAu = (await hole(1, 5)).find((e) => e.label === 'SOG AU');
        assert.ok(sogAu && sogAu.range, 'SOG AU hat keinen Bereich');
        const bereich = sogAu.range.start ? sogAu.range : sogAu.range.inserting;
        assert.deepStrictEqual([bereich.start.character, bereich.end.character], [0, 5]);
    },

    async 'Wer klein tippt, bekommt kleine Vorschläge und Bausteine'() {
        const hole = async (dokument, zeile, spalte) => {
            const liste = await vscode.commands.executeCommand(
                'vscode.executeCompletionItemProvider', dokument.uri, new vscode.Position(zeile, spalte));
            return liste.items.filter((e) => (e.detail || '').startsWith('OIDA'));
        };
        // Was eingesetzt würde: der eigene Text, sonst die Beschriftung.
        const text = (e) => {
            if (e.insertText) {
                return typeof e.insertText === 'string' ? e.insertText : e.insertText.value;
            }
            return typeof e.label === 'string' ? e.label : e.label.label;
        };
        const baustein = (liste, kuerzel) =>
            liste.find((e) => e.kind === vscode.CompletionItemKind.Snippet && e.filterText === kuerzel);

        // Klein getippt: Wörter und Bausteine kommen klein, Platzhalter wie "Name" bleiben.
        const klein = await oeffne(schreibe('klein.oida', 'so\nsog a\nwenn\nklasse\n'));
        const erste = (await hole(klein, 0, 2)).map(text);
        for (const wort of ['sog au', 'solong', 'passt scho']) {
            assert.ok(erste.includes(wort), `${wort} fehlt: ${erste.join(', ')}`);
        }
        assert.ok(!erste.includes('SOG AU') && !erste.includes('SOLONG'), `groß trotz klein getippt: ${erste.join(', ')}`);
        const sogAu = (await hole(klein, 1, 5)).find((e) => e.label === 'sog au');
        assert.ok(sogAu && sogAu.range, 'sog au hat keinen Bereich');
        const wenn = baustein(await hole(klein, 2, 4), 'wenn');
        assert.ok(wenn, 'der Baustein wenn fehlt');
        assert.strictEqual(text(wenn), 'wenn ${1:bedingung} daun\n\t$0\npasst scho');
        const klasse = text(baustein(await hole(klein, 3, 6), 'klasse'));
        assert.ok(klasse.startsWith('klasse ${1:Name}\n\t${2:feld} ${3:zoi}'), klasse);

        // Groß getippt: alles wie bisher.
        const gross = await oeffne(schreibe('gross.oida', 'SO\nWENN\n'));
        assert.ok((await hole(gross, 0, 2)).map(text).includes('SOG AU'));
        assert.strictEqual(text(baustein(await hole(gross, 1, 4), 'wenn')), 'WENN ${1:bedingung} DAUN\n\t$0\nPASST SCHO');

        // Noch nichts getippt: Es zählt, wie die Datei bisher geschrieben ist.
        const leer = await oeffne(schreibe('leer.oida', 'sog au a = 1 oida\nschreib(a) oida\n\n'));
        const amAnfang = (await hole(leer, 2, 0)).map(text);
        assert.ok(amAnfang.includes('wenn') && !amAnfang.includes('WENN'), `in einer klein geschriebenen Datei: ${amAnfang.slice(0, 8).join(', ')}`);

        // Die Einstellung geht vor.
        const einstellungen = vscode.workspace.getConfiguration('oida');
        await einstellungen.update('schreibweise', 'klein', vscode.ConfigurationTarget.Global);
        try {
            assert.ok((await hole(gross, 0, 2)).map(text).includes('sog au'), 'Einstellung klein, aber groß vorgeschlagen');
        } finally {
            await einstellungen.update('schreibweise', undefined, vscode.ConfigurationTarget.Global);
        }
        await einstellungen.update('schreibweise', 'GROSS', vscode.ConfigurationTarget.Global);
        try {
            assert.ok((await hole(klein, 0, 2)).map(text).includes('SOG AU'), 'Einstellung GROSS, aber klein vorgeschlagen');
        } finally {
            await einstellungen.update('schreibweise', undefined, vscode.ConfigurationTarget.Global);
        }
    },

    async 'Eigene Funktionen, Felder und Werte werden vorgeschlagen'() {
        // Die letzten drei Zeilen sind unfertig, so wie beim Tippen.
        const dokument = await oeffne(schreibe('namen.oida', [
            'BRAUCH mathe OIDA',
            '# Zählt zusammen.',
            'FUNKTIAUN zamm_zöhn(a ZOI, b ZOI) GIBT ZRUCK ZOI',
            '    GIB ZRUCK a + b OIDA',
            'PASST SCHO',
            'AUFZÖHLUNG Ampl',
            '    ROT, GRÜN',
            'PASST SCHO',
            'STRUKTUR Punkt',
            '    links ZOI',
            'PASST SCHO',
            'SOG AU eck = Punkt(1) OIDA',
            'za',
            'SOG AU a = Ampl.',
            'eck.',
            'SCHREIB(zamm())',
            '',
        ].join('\n')));
        const hole = async (zeile, spalte) => {
            const liste = await vscode.commands.executeCommand(
                'vscode.executeCompletionItemProvider', dokument.uri, new vscode.Position(zeile, spalte));
            return liste.items.filter((e) => e.kind !== vscode.CompletionItemKind.Text);
        };
        const name = (e) => (typeof e.label === 'string' ? e.label : e.label.label);
        const finde = (liste, wie) => liste.find((e) => name(e) === wie);
        const K = vscode.CompletionItemKind;

        // Mitten im Programm: die eigene Funktion mit ihren Parametern, eine aus der Bibliothek, Typen, Variablen.
        const frei = await hole(12, 2);
        const eigene = finde(frei, 'zamm_zöhn');
        assert.ok(eigene, `zamm_zöhn fehlt: ${frei.map(name).slice(0, 30).join(', ')}`);
        assert.strictEqual(eigene.kind, K.Function);
        assert.strictEqual(eigene.insertText.value, 'zamm_zöhn(${1:a}, ${2:b})');
        assert.strictEqual(eigene.detail, 'zamm_zöhn(a ZOI, b ZOI) GIBT ZRUCK ZOI');
        // (VS Code schreibt Leerzeichen in der Erklärung als &nbsp;.)
        assert.ok(eigene.documentation.value.replace(/&nbsp;/g, ' ').includes('Zählt zusammen.'), eigene.documentation.value);
        const klemm = finde(frei, 'klemm');
        assert.ok(klemm && klemm.label.description === 'mathe', 'klemm aus der Bibliothek mathe fehlt');
        assert.strictEqual(finde(frei, 'Punkt').kind, K.Class);
        assert.strictEqual(finde(frei, 'eck').kind, K.Variable);
        // Die Wörter der Sprache sind weiter dabei (klein, weil "za" klein getippt ist).
        assert.ok(finde(frei, 'sog au'), 'die Wörter der Sprache sind weg');
        assert.ok(!finde(frei, 'links') && !finde(frei, 'ROT'), 'Felder und Werte gehören hinter einen Punkt');
        // Was nicht dazugeholt ist, wird auch nicht vorgeschlagen.
        assert.ok(!finde(frei, 'sortiert'), 'sortiert kommt, obwohl listen nicht dazugeholt ist');

        // Hinter dem Namen einer Aufzählung: genau ihre Werte.
        const werte = await hole(13, 16);
        assert.deepStrictEqual(werte.map(name).sort(), ['GRÜN', 'ROT']);
        assert.strictEqual(werte[0].kind, K.EnumMember);

        // Hinter einem anderen Punkt: Felder (und Methoden), aber keine Funktionen und Wörter.
        const glieder = await hole(14, 4);
        assert.strictEqual(finde(glieder, 'links').kind, K.Field);
        assert.ok(!finde(glieder, 'zamm_zöhn') && !finde(glieder, 'SOG AU') && !finde(glieder, 'sog au'), glieder.map(name).join(', '));

        // Steht die Klammer schon da, kommt nur der Name.
        const mitKlammer = finde(await hole(15, 12), 'zamm_zöhn');
        assert.ok(mitKlammer, 'zamm_zöhn fehlt vor der Klammer');
        const eingesetzt = !mitKlammer.insertText ? name(mitKlammer)
            : (typeof mitKlammer.insertText === 'string' ? mitKlammer.insertText : mitKlammer.insertText.value);
        assert.strictEqual(eingesetzt, 'zamm_zöhn', 'trotz vorhandener Klammer kommen noch einmal Klammern');
    },

    async 'F5 übersetzt und startet das Programm'() {
        await oeffne(path.join(wurzel, 'beispiele', 'fizzbuzz.oida'));
        const exe = path.join(os.tmpdir(), 'oida-vscode', 'fizzbuzz.exe');
        fs.rmSync(exe, { force: true });
        assert.strictEqual(await laufenLassen('oida.starten'), 0);
        assert.ok(fs.existsSync(exe), `${exe} fehlt`);
        assert.ok(!fs.existsSync(path.join(wurzel, 'beispiele', 'fizzbuzz.exe')), 'die .exe liegt im Projektordner');
    },

    async 'Ein Programm, das abbricht, meldet Code 1'() {
        await oeffne(path.join(wurzel, 'uebersetzer', 'tests', 'laufzeitfehler', 'durch_null.oida'));
        assert.strictEqual(await laufenLassen('oida.starten'), 1);
    },

    async 'Ein Programm mit Fehler wird nicht gestartet'() {
        await oeffne(path.join(wurzel, 'uebersetzer', 'tests', 'fehler', 'falscher_typ.oida'));
        assert.strictEqual(await laufenLassen('oida.starten'), 1);
    },

    async 'Übersetzen legt die .exe neben die Datei'() {
        const datei = schreibe('servas.oida', 'SCHREIB("Servas") OIDA\n');
        await oeffne(datei);
        assert.strictEqual(await laufenLassen('oida.uebersetzen'), 0);
        assert.ok(fs.existsSync(path.join(arbeit, 'servas.exe')));
    },

    async 'Ungespeicherte Änderungen werden vor dem Starten gespeichert'() {
        const datei = schreibe('speichern.oida', 'SCHREIB("alt") OIDA\n');
        const dokument = await oeffne(datei);
        const aenderung = new vscode.WorkspaceEdit();
        aenderung.replace(dokument.uri, new vscode.Range(0, 9, 0, 12), 'neu');
        assert.ok(await vscode.workspace.applyEdit(aenderung));
        assert.ok(dokument.isDirty);
        assert.strictEqual(await laufenLassen('oida.starten'), 0);
        assert.strictEqual(fs.readFileSync(datei, 'utf8'), 'SCHREIB("neu") OIDA\n');
    },

    async 'Eigenes Wörterbuch über die Einstellung'() {
        const woerterbuch = schreibe('meins.txt', 'IF = FOIS\nOIDA = HAWARA\nPRINT = ZAG\n');
        const einstellungen = vscode.workspace.getConfiguration('oida');
        await einstellungen.update('woerterbuch', woerterbuch, vscode.ConfigurationTarget.Global);
        try {
            const dokument = await oeffne(schreibe('eigen.oida', 'ZAG(1) HAWARA\nZAG(2) OIDA\n'));
            const fehler = await bis('Fehler mit eigenem Wörterbuch', () => {
                const liste = vscode.languages.getDiagnostics(dokument.uri);
                return liste.length > 0 ? liste : null;
            });
            // OIDA ist jetzt kein Wort der Sprache mehr, der Compiler sagt, wie es heißt.
            assert.strictEqual(fehler[0].message, 'Des haaßt HAWARA, ned OIDA!');
            assert.strictEqual(fehler[0].range.start.line, 1);
            const liste = await tokens(dokument);
            assert.deepStrictEqual(liste.slice(0, 3), [[0, 0, 3, 'keyword'], [0, 4, 1, 'number'], [0, 7, 6, 'endwort']]);
        } finally {
            await einstellungen.update('woerterbuch', undefined, vscode.ConfigurationTarget.Global);
        }
    },
};

exports.run = async function run() {
    let fehlgeschlagen = 0;
    for (const [name, pruefung] of Object.entries(pruefungen)) {
        try {
            await pruefung();
            bericht.push(`ok      ${name}`);
        } catch (e) {
            fehlgeschlagen++;
            bericht.push(`FEHLER  ${name}\n        ${String(e && e.stack || e).split('\n').join('\n        ')}`);
        }
    }
    bericht.push('', `${Object.keys(pruefungen).length - fehlgeschlagen} von ${Object.keys(pruefungen).length} bestanden`);
    const text = bericht.join('\n') + '\n';
    console.log(text);
    if (process.env.OIDA_TESTBERICHT) {
        fs.writeFileSync(process.env.OIDA_TESTBERICHT, text, 'utf8');
    }
    try {
        fs.rmSync(arbeit, { recursive: true, force: true });
    } catch {
        // Das Terminal steht noch in dem Ordner; Windows räumt den Temp-Ordner später selbst auf.
    }
    if (fehlgeschlagen > 0) {
        throw new Error(`${fehlgeschlagen} Prüfungen fehlgeschlagen`);
    }
};
