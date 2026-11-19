# Baut die Installer fuer OIDA. Sie landen in installer\aus\.
#
#   bauen.ps1            der Windows-Installer:  oida-<version>-windows-x86_64-setup.exe
#   bauen.ps1 -Linux     dazu die beiden fuer Linux, gebaut im WSL:
#                        oida-<version>-linux-x86_64.sh  und  oida_<version>_amd64.deb
#   bauen.ps1 -Linux -Distribution Debian     ein anderes WSL-Linux als "Ubuntu" nehmen
#
# Was passiert:
#   1. Compiler und VS-Code-Erweiterung werden gebaut (vscode\bauen.ps1).
#   2. Alles, was installiert wird, kommt in einen Ordner: oida.exe, Beispiele,
#      die Erweiterung, LIESMICH.txt, das Woerterbuch als Vorlage.
#   3. Der Installer wird gebaut und packt diesen Ordner in sich hinein
#      (installer\windows\build.rs).
#
# Diese Datei absichtlich ohne Umlaute (Windows PowerShell liest sie sonst falsch).
param([switch]$Linux, [string]$Distribution = 'Ubuntu')

$ErrorActionPreference = 'Stop'
$hier   = Split-Path -Parent $MyInvocation.MyCommand.Path
$wurzel = Split-Path -Parent $hier
$aus    = Join-Path $hier 'aus'

$cargo = (Get-Command 'cargo.exe' -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe" }
if (-not (Test-Path $cargo)) { throw 'cargo nicht gefunden. Rust installieren (siehe README.md).' }

$zeile = Get-Content (Join-Path $wurzel 'Cargo.toml') -Encoding UTF8 | Where-Object { $_ -match '^version\s*=\s*"(.+)"' } | Select-Object -First 1
if (-not ($zeile -match '"(.+)"')) { throw 'Die Version steht nicht in Cargo.toml.' }
$version = $Matches[1]

# --- 1. und 2. Was installiert wird ------------------------------------------
$inhalt = Join-Path $wurzel 'target\installer\inhalt'
if (Test-Path $inhalt) { Remove-Item -Recurse -Force $inhalt }
New-Item -ItemType Directory -Force (Join-Path $inhalt 'beispiele') | Out-Null
New-Item -ItemType Directory -Force $aus | Out-Null

# Die Erweiterung ohne den Ort des Compilers auf diesem Rechner: Sie sucht ihn dann im PATH.
& (Join-Path $wurzel 'vscode\bauen.ps1') -OhneOrt -Ziel (Join-Path $inhalt 'oida-vscode.vsix')
if (-not (Test-Path (Join-Path $inhalt 'oida-vscode.vsix'))) { throw 'Die VS-Code-Erweiterung fehlt.' }

Copy-Item (Join-Path $wurzel 'target\release\oida.exe') $inhalt
Copy-Item (Join-Path $hier 'LIESMICH.txt') $inhalt
Copy-Item (Join-Path $wurzel 'woerterbuch.txt') (Join-Path $inhalt 'woerterbuch-vorlog.txt')
# Beispiele: die Programme selbst, ohne die erwarteten Ausgaben der Tests und ohne
# das grosse Spiel mit seiner Musik (das liegt im Projekt).
Get-ChildItem (Join-Path $wurzel 'beispiele') -Filter '*.oida' | Copy-Item -Destination (Join-Path $inhalt 'beispiele')
foreach ($ordner in @('turnier', 'spui', 'netz')) {
    $ziel = Join-Path $inhalt "beispiele\$ordner"
    New-Item -ItemType Directory -Force $ziel | Out-Null
    Get-ChildItem (Join-Path $wurzel "beispiele\$ordner") -File | Where-Object { $_.Extension -ne '.erwartet' } | Copy-Item -Destination $ziel
}

# --- 3. Der Installer ---------------------------------------------------------
Write-Host 'Baue den Windows-Installer ...'
$env:OIDA_SETUP_INHALT = $inhalt
try {
    & $cargo build --release -p oida-setup --manifest-path (Join-Path $wurzel 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'Der Installer laesst sich nicht bauen.' }
} finally {
    Remove-Item Env:\OIDA_SETUP_INHALT
}
$setup = Join-Path $aus "oida-$version-windows-x86_64-setup.exe"
Copy-Item -Force (Join-Path $wurzel 'target\release\oida-setup.exe') $setup
# Die Erweiterung auch einzeln: Der Linux-Bau nimmt sie von hier.
Copy-Item -Force (Join-Path $inhalt 'oida-vscode.vsix') (Join-Path $aus 'oida-vscode.vsix')
Write-Host ("Fertig: {0} ({1:N1} MB)" -f $setup, ((Get-Item $setup).Length / 1MB))

# --- Auf Wunsch die Installer fuer Linux --------------------------------------
if ($Linux) {
    $wsl = (Get-Command 'wsl.exe' -ErrorAction SilentlyContinue).Source
    if (-not $wsl) { throw 'WSL nicht gefunden. Die Linux-Installer baut man sonst unter Linux mit installer/linux/bauen.sh.' }
    # Den Bauordner legt bauen.sh ins Linux selbst (~/.cache/oida-bau): Im Windows-Ordner
    # waere es langsam, und die beiden Systeme kaemen sich in target\ in die Quere.
    $skript = (& $wsl -d $Distribution -- wslpath -a ((Join-Path $hier 'linux\bauen.sh') -replace '\\', '/')).Trim()
    Write-Host "Baue die Linux-Installer im WSL ($Distribution) ..."
    & $wsl -d $Distribution -- bash $skript
    if ($LASTEXITCODE -ne 0) { throw 'Die Linux-Installer lassen sich nicht bauen.' }
}
