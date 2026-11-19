# Probiert den Windows-Installer von Anfang bis Ende aus (vorher bauen.ps1).
#
#   testen.ps1            alles ausser dem echten Herunterladen
#   testen.ps1 -Laden     dazu holt der Installer llvm-mingw wirklich von GitHub (190 MB)
#                         und das installierte OIDA uebersetzt damit ein Programm
#
# Dein Rechner bleibt dabei, wie er ist:
#   - LOCALAPPDATA zeigt fuer den Installer in einen Probe-Ordner unter %TEMP%,
#   - PATH und der Eintrag unter "Apps" landen unter HKCU\Software\OIDA-Setup-Probe
#     statt an den echten Stellen (OIDA_SETUP_TESTSCHLUESSEL),
#   - llvm-mingw kommt aus einer winzigen nachgemachten Zip-Datei (ausser mit -Laden).
# Am Ende sind Probe-Ordner und Test-Schluessel wieder weg.
#
# Diese Datei absichtlich ohne Umlaute (Windows PowerShell liest sie sonst falsch).
param([switch]$Laden)

$ErrorActionPreference = 'Stop'
$hier  = Split-Path -Parent $MyInvocation.MyCommand.Path
$setup = Get-ChildItem (Join-Path $hier 'aus') -Filter 'oida-*-windows-x86_64-setup.exe' | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $setup) { throw 'Kein Installer in installer\aus. Zuerst bauen.ps1 laufen lassen.' }
$setup = $setup.FullName

$probe      = Join-Path $env:TEMP 'oida-setup-probe'
$schluessel = 'Software\OIDA-Setup-Probe'
$echtesLokal = $env:LOCALAPPDATA
$script:geprueft = 0

function Reg($befehl) { & cmd.exe /c "reg.exe $befehl 2>&1" | Where-Object { $_.Trim() } }
function Pruef($bedingung, $was) {
    if (-not $bedingung) { throw "FALSCH: $was" }
    $script:geprueft++
    Write-Host "  ok  $was"
}
# Startet ein Programm ohne Eingabe (niemand da zum Antworten) und gibt Code und Ausgabe zurueck.
function Lauf($programm, $argumente) {
    $aus = Join-Path $probe 'ausgabe.txt'
    $p = Start-Process -FilePath $programm -ArgumentList $argumente -NoNewWindow -Wait -PassThru `
        -RedirectStandardInput (Join-Path $probe 'laar.txt') -RedirectStandardOutput $aus -RedirectStandardError (Join-Path $probe 'fehler.txt')
    $text = (Get-Content $aus -Raw -Encoding UTF8) + (Get-Content (Join-Path $probe 'fehler.txt') -Raw -Encoding UTF8)
    return @{ Code = $p.ExitCode; Text = "$text" }
}
function Pfad-Wert { (Reg "query HKCU\$schluessel\Environment /v Path" | Where-Object { $_ -match '^\s+Path\s' }) -replace '^\s+Path\s+REG_\w+\s*', '' }

try {
    if (Test-Path $probe) { Remove-Item -Recurse -Force $probe }
    New-Item -ItemType Directory -Force $probe | Out-Null
    Set-Content -Path (Join-Path $probe 'laar.txt') -Value '' -NoNewline
    Reg "delete HKCU\$schluessel /f" | Out-Null

    # Eine nachgemachte llvm-mingw-Zip: ein Ordner mit bin\clang.exe drin.
    $nach = Join-Path $probe 'nachgemacht\llvm-mingw-probe\bin'
    New-Item -ItemType Directory -Force $nach | Out-Null
    Set-Content -Path (Join-Path $nach 'clang.exe') -Value 'des is ka echts clang' -Encoding ascii
    $zip = Join-Path $probe 'llvm-probe.zip'
    & "$env:SystemRoot\System32\tar.exe" -a -cf $zip -C (Join-Path $probe 'nachgemacht') 'llvm-mingw-probe'
    $summe = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()

    $lokal  = Join-Path $probe 'lokal'
    $ordner = Join-Path $lokal 'Programs\oida'
    $llvm   = Join-Path $lokal 'Programs\llvm-mingw'
    $env:LOCALAPPDATA = $lokal
    $env:OIDA_SETUP_TESTSCHLUESSEL = $schluessel
    $env:OIDA_SETUP_LLVM_QUELLE = 'file:///' + ($zip -replace '\\', '/')
    Remove-Item Env:\OIDA_CLANG -ErrorAction SilentlyContinue

    Write-Host '1. Falsche Pruefsumme: Die Datei wird nicht genommen'
    $env:OIDA_SETUP_LLVM_SHA256 = '00' * 32
    $l = Lauf $setup @('--jo', '--ohne-vscode')
    Pruef ($l.Code -eq 3) 'Code 3: installiert, aber clang fehlt'
    Pruef ($l.Text -match 'Pr.fsumme passt ned') 'die Meldung nennt die Pruefsumme'
    Pruef (-not (Test-Path $llvm) -and -not (Test-Path ($llvm + '.teil'))) 'kein llvm-Ordner, auch kein halber'
    Pruef (Test-Path (Join-Path $ordner 'oida.exe')) 'OIDA selbst ist trotzdem installiert'
    $env:OIDA_SETUP_LLVM_SHA256 = $summe

    Write-Host '2. Niemand da zum Antworten und kein --jo: Es wird nichts geholt'
    Remove-Item -Recurse -Force $ordner
    Reg "delete HKCU\$schluessel /f" | Out-Null
    $l = Lauf $setup @('--ohne-vscode')
    Pruef ($l.Code -eq 0) 'Code 0'
    Pruef ($l.Text -match 'kana do zum Frogn') 'die Frage wird als nein gewertet'
    Pruef (-not (Test-Path $llvm)) 'kein llvm-Ordner'
    Pruef ((Pfad-Wert) -eq $ordner) 'der PATH hat den Ordner bekommen (das fragt er nicht)'

    Write-Host '3. Installieren mit --jo'
    $l = Lauf $setup @('--jo', '--ohne-vscode')
    Pruef ($l.Code -eq 0 -and $l.Text -match 'Passt! OIDA is installiert') 'Code 0 und die Erfolgsmeldung'
    Pruef (Test-Path (Join-Path $llvm 'bin\clang.exe')) 'llvm-mingw ist ausgepackt, ohne den Ordner aus der Zip-Datei'
    foreach ($datei in @('oida.exe', 'deinstalliern.exe', 'installiert.txt', 'LIESMICH.txt', 'oida-vscode.vsix', 'woerterbuch-vorlog.txt', 'beispiele\hallo.oida', 'beispiele\turnier\spiela.oida')) {
        Pruef (Test-Path (Join-Path $ordner $datei)) "$datei liegt da"
    }
    $eintrag = Reg "query HKCU\$schluessel\Uninstall\OIDA"
    Pruef (($eintrag -join "`n") -match 'DisplayName\s+REG_SZ\s+OIDA') 'der Eintrag unter "Apps" hat seinen Namen'
    Pruef (($eintrag -join "`n") -match 'UninstallString\s+REG_SZ\s+".*deinstalliern\.exe" --weg') '... und weiss, wie man OIDA entfernt'

    Write-Host '4. Noch einmal installieren: nichts doppelt'
    $l = Lauf $setup @('--jo', '--ohne-vscode')
    Pruef ($l.Text -match 'steht scho drin' -and $l.Text -match 'is scho do') 'PATH und clang werden wiedererkannt'
    Pruef ((Pfad-Wert) -eq $ordner) 'der Ordner steht genau einmal im PATH'

    Write-Host '5. Der installierte Compiler'
    $l = Lauf (Join-Path $ordner 'oida.exe') @('--version')
    Pruef ($l.Text -match '^oida \d+\.\d+\.\d+') 'oida --version'
    $env:OIDA_CLANG = Join-Path $echtesLokal 'Programs\llvm-mingw\bin\clang.exe'
    if (Test-Path $env:OIDA_CLANG) {
        $l = Lauf (Join-Path $ordner 'oida.exe') @((Join-Path $ordner 'beispiele\hallo.oida'), '-o', (Join-Path $probe 'hallo.exe'), '--renn')
        Pruef ($l.Code -eq 0 -and $l.Text -match 'Servas') 'ein Beispiel aus dem Installer laesst sich uebersetzen und laeuft'
    } else {
        Write-Host '  (ausgelassen: Auf diesem Rechner gibt es kein echtes llvm-mingw zum Uebersetzen)'
    }
    Remove-Item Env:\OIDA_CLANG

    Write-Host '6. Entfernen laesst liegen, was nicht von OIDA ist'
    # Zwei eigene Dateien: eine ganz vorn im Ordner (vor allem, was OIDA hingelegt hat) und eine weiter innen.
    Set-Content -Path (Join-Path $ordner 'aaa-meins.txt') -Value 'des ghoert mia'
    Set-Content -Path (Join-Path $ordner 'beispiele\meins.oida') -Value 'des a'
    $l = Lauf (Join-Path $ordner 'deinstalliern.exe') @('--weg', '--jo')
    Pruef ($l.Code -eq 0 -and $l.Text -match 'OIDA is weg') 'Code 0 und die Meldung'
    Start-Sleep -Seconds 5
    $rest = @(Get-ChildItem -Recurse -File $ordner | ForEach-Object { $_.Name } | Sort-Object)
    Pruef (($rest -join ' ') -eq 'aaa-meins.txt meins.oida') 'nur die zwei eigenen Dateien sind noch da (auch der Entferner selbst ist weg)'
    Pruef ((Pfad-Wert) -eq '') 'der Ordner ist aus dem PATH'
    Pruef (-not ((Reg "query HKCU\$schluessel\Uninstall\OIDA") -match 'DisplayName')) 'der Eintrag unter "Apps" ist weg'
    Pruef (Test-Path (Join-Path $llvm 'bin\clang.exe')) 'llvm-mingw bleibt'

    Write-Host '7. Entfernen, wo nichts installiert ist'
    Remove-Item -Recurse -Force $ordner
    $l = Lauf $setup @('--weg', '--jo')
    Pruef ($l.Code -eq 1 -and $l.Text -match 'is ka OIDA installiert') 'Code 1 und eine Meldung'

    if ($Laden) {
        Write-Host '8. Echt: llvm-mingw von GitHub holen (190 MB) und damit uebersetzen'
        Remove-Item -Recurse -Force $llvm
        Remove-Item Env:\OIDA_SETUP_LLVM_QUELLE, Env:\OIDA_SETUP_LLVM_SHA256
        $l = Lauf $setup @('--jo', '--ohne-vscode', '--ohne-pfad')
        Pruef ($l.Code -eq 0) 'Code 0'
        Pruef (Test-Path (Join-Path $llvm 'bin\clang.exe')) 'clang ist da'
        # Ohne OIDA_CLANG: Der Compiler findet es von selbst unter LOCALAPPDATA\Programs\llvm-mingw.
        $l = Lauf (Join-Path $ordner 'oida.exe') @((Join-Path $ordner 'beispiele\fizzbuzz.oida'), '-o', (Join-Path $probe 'fizzbuzz.exe'), '--renn')
        Pruef ($l.Code -eq 0 -and $l.Text -match 'FizzBuzz') 'das installierte OIDA uebersetzt mit dem geholten clang'
    }
    Write-Host ''
    Write-Host "$($script:geprueft) von $($script:geprueft) bestanden"
} finally {
    $env:LOCALAPPDATA = $echtesLokal
    Remove-Item Env:\OIDA_SETUP_TESTSCHLUESSEL, Env:\OIDA_SETUP_LLVM_QUELLE, Env:\OIDA_SETUP_LLVM_SHA256, Env:\OIDA_CLANG -ErrorAction SilentlyContinue
    Reg "delete HKCU\$schluessel /f" | Out-Null
    if (Test-Path $probe) { Remove-Item -Recurse -Force $probe -ErrorAction SilentlyContinue }
}
