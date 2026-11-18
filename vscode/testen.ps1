# Testet die VS-Code-Erweiterung in einem echten VS Code.
#
#   testen.ps1         testet den Quellordner (vscode\)
#   testen.ps1 -Vsix   testet die fertig gepackte oida-vscode.vsix (vorher bauen.ps1)
#   testen.ps1 -Vsix -FremderOrdner
#                      wie -Vsix, aber geoeffnet ist ein Ordner ohne target\: die Erweiterung
#                      muss den Compiler ueber den beim Bauen gemerkten Ort finden
#
# Es geht kurz ein eigenes VS-Code-Fenster auf und wieder zu. Es verwendet
# eigene, leere Einstellungen; dein normales VS Code bleibt unberuehrt.
# Die Pruefungen stehen in test\lauf.js.
#
# Diese Datei absichtlich ohne Umlaute (Windows PowerShell liest sie sonst falsch).
param([switch]$Vsix, [switch]$FremderOrdner)

$ErrorActionPreference = 'Stop'
$hier   = Split-Path -Parent $MyInvocation.MyCommand.Path
$wurzel = Split-Path -Parent $hier

$orte = @(
    "$env:LOCALAPPDATA\Programs\Microsoft VS Code",
    "$env:ProgramFiles\Microsoft VS Code"
)
$vscode = $orte | Where-Object { Test-Path (Join-Path $_ 'Code.exe') } | Select-Object -First 1
if (-not $vscode) { throw 'VS Code nicht gefunden. Installieren mit: winget install Microsoft.VisualStudioCode' }
if (-not (Test-Path (Join-Path $wurzel 'target\release\oida.exe')) -and -not (Test-Path (Join-Path $wurzel 'target\debug\oida.exe'))) {
    throw 'Der Compiler ist noch nicht gebaut. Zuerst: cargo build --release'
}

$arbeit = Join-Path ([System.IO.Path]::GetTempPath()) ("oida-vscode-lauf-" + [System.Guid]::NewGuid().ToString('N').Substring(0, 8))
$daten  = Join-Path $arbeit 'daten'
$erw    = Join-Path $arbeit 'erweiterungen'
New-Item -ItemType Directory -Force $daten, $erw | Out-Null
$bericht = Join-Path $arbeit 'bericht.txt'

# Unter test-electron & Co. laeuft Code.exe sonst als blosses Node.
Remove-Item Env:ELECTRON_RUN_AS_NODE -ErrorAction SilentlyContinue
$env:OIDA_TESTBERICHT = $bericht

if ($Vsix) {
    $paket = Join-Path $hier 'oida-vscode.vsix'
    if (-not (Test-Path $paket)) { throw 'oida-vscode.vsix gibt es noch nicht. Zuerst bauen.ps1 ausfuehren.' }
    & (Join-Path $vscode 'bin\code.cmd') --user-data-dir $daten --extensions-dir $erw --install-extension $paket --force
    if ($LASTEXITCODE -ne 0) { throw 'Die .vsix-Datei laesst sich nicht installieren.' }
    # Als "Erweiterung in Entwicklung" dient eine leere Huelle; getestet wird die installierte.
    $entwicklung = Join-Path $arbeit 'huelle'
    New-Item -ItemType Directory -Force $entwicklung | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $entwicklung 'package.json'),
        '{ "name": "huelle", "publisher": "test", "version": "0.0.1", "engines": { "vscode": "^1.85.0" } }')
} else {
    $entwicklung = $hier
}

$offen = $wurzel
if ($FremderOrdner) {
    if (-not $Vsix) { throw '-FremderOrdner geht nur zusammen mit -Vsix.' }
    # Nur die Programme kopieren, die die Pruefungen brauchen; kein target\.
    $offen = Join-Path $arbeit 'projekt'
    New-Item -ItemType Directory -Force (Join-Path $offen 'uebersetzer') | Out-Null
    Copy-Item -Recurse (Join-Path $wurzel 'beispiele') (Join-Path $offen 'beispiele')
    Copy-Item -Recurse (Join-Path $wurzel 'uebersetzer\tests') (Join-Path $offen 'uebersetzer\tests')
}

$argumente = @(
    "--extensionDevelopmentPath=`"$entwicklung`"",
    "--extensionTestsPath=`"$(Join-Path $hier 'test\lauf.js')`"",
    "--user-data-dir=`"$daten`"",
    "--extensions-dir=`"$erw`"",
    '--disable-workspace-trust',
    '--skip-welcome',
    '--skip-release-notes',
    '--disable-updates',
    "`"$offen`""
)
$lauf = Start-Process -FilePath (Join-Path $vscode 'Code.exe') -ArgumentList $argumente -Wait -PassThru

$gelaufen = Test-Path $bericht
if ($gelaufen) {
    Get-Content $bericht -Encoding UTF8 | Write-Host
} else {
    Write-Host 'Kein Bericht: die Pruefungen sind gar nicht gelaufen.'
}
$code = $lauf.ExitCode
Remove-Item -Recurse -Force $arbeit -ErrorAction SilentlyContinue
if ($code -ne 0 -or -not $gelaufen) {
    Write-Host "VS Code hat mit Code $code aufgehoert."
    exit 1
}
exit 0
