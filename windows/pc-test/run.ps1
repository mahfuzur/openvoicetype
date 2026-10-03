# OpenVoiceType for Windows: the W1 test on a real PC (docs/plans/M8-windows.md).
#
# Run it from the folder you unzipped the "OpenVoiceType-windows" CI artifact into (OpenVoiceType.exe, ovt.exe,
# prompts\, evals\ and this script), in PowerShell:
#
#     powershell -ExecutionPolicy Bypass -File .\pc-test\run.ps1
#
# It downloads the pinned Whisper and llama.cpp programs and the speech model (once), checks Claude Code, runs the
# pipeline on synthesized speech, runs the evals if Python is installed, then walks you through a few dictations.
# Everything goes into one report on your Desktop: send that file back. Nothing is uploaded anywhere.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue' # Invoke-WebRequest is very slow with the progress bar
$Root = Split-Path -Parent $PSScriptRoot
$Report = Join-Path ([Environment]::GetFolderPath('Desktop')) 'openvoicetype-windows-test.txt'
$Lines = New-Object System.Collections.Generic.List[string]

function Log([string]$Text = '') { Write-Host $Text; $Lines.Add($Text) }
function Result([string]$Name, [bool]$Ok, [string]$Detail = '') {
    $mark = if ($Ok) { 'ok  ' } else { 'FAIL' }
    Log ("$mark $Name" + $(if ($Detail) { ": $Detail" } else { '' }))
}
function Ask([string]$Question) {
    $answer = Read-Host "`n>>> $Question [y/n]"
    return $answer.Trim().ToLower().StartsWith('y')
}
function Save { $Lines | Set-Content -Encoding UTF8 $Report }

# Pinned downloads (SHA-256 checked): the official whisper.cpp and llama.cpp Windows builds (CPU), and the same
# compressed Whisper model as the Mac app (ModelManager.swift, install.sh).
$Downloads = @(
    @{ Name = 'whisper.cpp b5130 (CPU)'; Url = 'https://github.com/ggml-org/whisper.cpp/releases/download/b5130/whisper-bin-x64.zip'
       Sha = 'f9ec6c52a2e949b62ab51fa21d0d497958f9e41c3010c157c4e42932d5316f3c'; Dir = 'helpers\whisper' },
    @{ Name = 'llama.cpp b11371 (CPU)'; Url = 'https://github.com/ggml-org/llama.cpp/releases/download/b11371/llama-b11371-bin-win-cpu-x64.zip'
       Sha = '085d650f1b4b0725a32c2421b970da5c5d2f769202cd16f89591ac9dcce835c6'; Dir = 'helpers\llama' }
)
$ModelDir = Join-Path $env:LOCALAPPDATA 'voice-to-text\whisper'
$Model = Join-Path $ModelDir 'ggml-large-v3-turbo-q5_0.bin'
$ModelUrl = 'https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo-q5_0.bin'
$ModelSha = '394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2'

function Get-Checked([string]$Url, [string]$File, [string]$Sha) {
    if ((Test-Path $File) -and ((Get-FileHash -Algorithm SHA256 $File).Hash.ToLower() -eq $Sha)) { return }
    Write-Host "    downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile "$File.part" -UseBasicParsing
    $hash = (Get-FileHash -Algorithm SHA256 "$File.part").Hash.ToLower()
    if ($hash -ne $Sha) { Remove-Item "$File.part"; throw "Checksum mismatch for $Url ($hash)" }
    Move-Item -Force "$File.part" $File
}

Log "OpenVoiceType Windows test report, $(Get-Date -Format 'yyyy-MM-dd HH:mm')"
$os = Get-CimInstance Win32_OperatingSystem
$cpu = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
$gpus = (Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }) -join '; '
$ram = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1)
Log "system: $($os.Caption) $($os.Version) build $($os.BuildNumber) / $cpu / $ram GB / GPU: $gpus"
Log "keyboard layouts: $((Get-WinUserLanguageList | ForEach-Object { $_.LanguageTag }) -join ', ')"
Log "artifact: $Root"
Result 'OpenVoiceType.exe and ovt.exe present' ((Test-Path "$Root\OpenVoiceType.exe") -and (Test-Path "$Root\ovt.exe"))

# 1. Helpers and model.
Log "`n# 1. Whisper, llama.cpp and the speech model"
foreach ($d in $Downloads) {
    $dir = Join-Path $Root $d.Dir
    $zip = Join-Path $Root (($d.Dir -replace '\\', '-') + '.zip')
    if (-not (Test-Path $dir)) {
        Get-Checked $d.Url $zip $d.Sha
        Expand-Archive -Force $zip -DestinationPath $dir
        # whisper.cpp's zip has everything in Release\.
        if (Test-Path "$dir\Release") { Move-Item "$dir\Release\*" $dir; Remove-Item "$dir\Release" }
        Remove-Item $zip
    }
    Result "$($d.Name) installed" $true $dir
}
New-Item -ItemType Directory -Force $ModelDir | Out-Null
if (-not (Test-Path $Model)) { Write-Host "    the speech model is 574 MB, this takes a few minutes" }
Get-Checked $ModelUrl $Model $ModelSha
Result 'speech model' (Test-Path $Model) $Model
Save

# 2. Claude Code.
Log "`n# 2. Claude Code"
$claude = @("$env:USERPROFILE\.local\bin\claude.exe") + @((Get-Command claude.exe, claude.cmd -ErrorAction SilentlyContinue).Source) |
    Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($claude) {
    $version = & $claude --version 2>&1
    $status = & $claude auth status --json 2>&1 | Out-String
    Result 'claude found' $true "$claude ($version)"
    Result 'claude signed in' ($status -match '"loggedIn":\s*true') (($status -replace '\s+', ' ').Substring(0, [math]::Min(160, $status.Length)))
} else {
    Result 'claude found' $false 'install it: irm https://claude.ai/install.ps1 | iex   then: claude auth login'
}
Save

# 3. The pipeline on synthesized speech (the Windows voice), with whisper-cli and then the warm whisper-server.
Log "`n# 3. The pipeline (ovt selftest)"
$env:VTT_BIN_DIR = Join-Path $Root 'helpers\whisper'
$env:VTT_WHISPER_MODEL = $Model
$env:VTT_PROMPTS_DIR = Join-Path $Root 'prompts'
if ($claude) { $env:VTT_CLAUDE_BIN = $claude }
$out = & "$Root\ovt.exe" selftest 2>&1 | Out-String
Log $out.Trim()
Result 'selftest with whisper-cli' ($LASTEXITCODE -eq 0 -and $out -match 'cleaned:')
$server = Start-Process -PassThru -WindowStyle Hidden -FilePath "$Root\helpers\whisper\whisper-server.exe" `
    -ArgumentList @('-m', $Model, '--host', '127.0.0.1', '--port', '8179', '-nt', '-sns', '-l', 'en')
$ready = $false
foreach ($i in 1..60) {
    try { if ((Invoke-WebRequest -UseBasicParsing -TimeoutSec 1 'http://127.0.0.1:8179/health').Content -match 'ok') { $ready = $true; break } } catch { }
    Start-Sleep -Milliseconds 500
}
Result 'whisper-server started' $ready "after $([math]::Round($i / 2, 1)) s"
foreach ($run in 1..2) {
    $out = & "$Root\ovt.exe" selftest 2>&1 | Out-String
    Log $out.Trim()
}
Result 'selftest with whisper-server' ($LASTEXITCODE -eq 0 -and $out -match 'cleaned:')
Save

# 4. The evals (needs Python 3 and a signed-in Claude).
Log "`n# 4. Evals"
$python = (Get-Command python.exe, py.exe -ErrorAction SilentlyContinue | Select-Object -First 1).Source
if ($python -and $claude -and (Test-Path "$Root\evals\run.py")) {
    $out = & $python "$Root\evals\run.py" --bin "$Root\ovt.exe" --jobs 2 2>&1 | Out-String
    Log $out.Trim()
    $out = & $python "$Root\evals\run.py" --bin "$Root\ovt.exe" --command --jobs 2 2>&1 | Out-String
    Log $out.Trim()
} else {
    Log 'skipped (needs python and claude)'
}
Stop-Process -Id $server.Id -ErrorAction SilentlyContinue
Save

# 5. The app's own checks, then real dictations.
Log "`n# 5. The app"
$selftest = Join-Path $env:TEMP 'openvoicetype-logic-selftest.txt'
& "$Root\OpenVoiceType.exe" --logic-selftest $selftest | Out-Null
Start-Sleep -Seconds 2
if (Test-Path $selftest) { Get-Content $selftest | ForEach-Object { Log "     $_" } }
Result 'logic selftest' ((Test-Path $selftest) -and -not (Select-String -Quiet -Pattern '^FAIL' $selftest))

Write-Host "`nNow the real app. It starts in the notification area (the icons next to the clock; Windows may hide it under ^)."
Start-Process "$Root\OpenVoiceType.exe"
Start-Sleep -Seconds 3
Result 'tray icon visible' (Ask 'Do you see the OpenVoiceType icon near the clock (maybe under the ^ arrow)?')
Write-Host "`nOpen Notepad, click in it, press Ctrl+Alt+Space, say a sentence with a number (""the budget is 15,400 dollars""),"
Write-Host "and press Ctrl+Alt+Space again."
Result 'dictation pasted into Notepad' (Ask 'Did the cleaned-up sentence appear in Notepad?')
Result 'overlay shown while recording' (Ask 'Did a dark pill show Recording / Transcribing / Polishing at the bottom?')
Write-Host "`nHold to talk: in Notepad, HOLD Ctrl+Alt+Space while you speak, and let go to finish (turn on Hold to Talk in the tray menu first if it's there)."
Result 'hold to talk' (Ask 'Did it record while held and paste after you let go?')
Write-Host "`nEsc: start a dictation, then press Esc."
Result 'Esc cancels' (Ask 'Did Esc cancel it (nothing pasted)?')
Write-Host "`nWindows Terminal: click in a terminal window, dictate 'echo hello world'."
Result 'paste in Windows Terminal' (Ask 'Did the text appear at the prompt (not run)?')
Write-Host "`nPassword field: open Edge at https://github.com/login, click the password box, dictate anything."
Result 'password field refused' (Ask 'Did it refuse (nothing typed into the password box)?')
Write-Host "`nAdmin window: start Notepad as administrator (right-click > Run as administrator), click in it, dictate."
Result 'admin window: copied, not pasted' (Ask 'Did it say Copied (and Ctrl+V pastes it)?')
Write-Host "`nChanged window: start a dictation in Notepad, switch to another window before it finishes."
Result 'changed window: copied' (Ask 'Did it copy instead of pasting into the other window?')
Result 'clipboard restored' (Ask 'After a dictation, does Ctrl+V still paste what you had copied before?')
Result 'not in clipboard history' (Ask 'Press Win+V: is the dictated text absent from the clipboard history?')

$log = Join-Path $env:LOCALAPPDATA 'voice-to-text\logs\dictate.log'
if (Test-Path $log) {
    Log "`n# dictate.log (last 40 lines; no dictated text unless you turned text logging on)"
    Get-Content -Tail 40 $log | ForEach-Object { Log "     $_" }
}
Save
Write-Host "`nDone. Report: $Report  (send it back)"
