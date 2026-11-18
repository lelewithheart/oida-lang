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
