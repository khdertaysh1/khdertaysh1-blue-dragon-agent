param()
$ErrorActionPreference = 'Stop'
$tag = 'v1.1.14.4'
$assets = "https://github.com/khdertaysh1/khdertaysh1-blue-dragon-agent/releases/download/$tag"
$name = 'blue-dragon-windows-x64.exe'
$dir = Join-Path $env:LOCALAPPDATA 'BlueDragon\bin'
[void][IO.Directory]::CreateDirectory($dir)
$stage = Join-Path $dir ([Guid]::NewGuid().ToString())
[void][IO.Directory]::CreateDirectory($stage)
try {
    $checks = (Invoke-WebRequest -UseBasicParsing "$assets/SHA256SUMS.txt").Content
    if ($checks -is [byte[]]) { $checks = [Text.Encoding]::UTF8.GetString($checks) }
    foreach ($item in @($name, 'LICENSES.txt')) {
        $path = Join-Path $stage $item
        $pattern = '(?m)^([0-9a-f]{64})  ' + [regex]::Escape($item) + '\r?$'
        $rows = [regex]::Matches($checks, $pattern)
        if ($rows.Count -ne 1) { throw "Missing or ambiguous checksum for $item" }
        Invoke-WebRequest -UseBasicParsing "$assets/$item" -OutFile $path
        if ((Get-FileHash $path -Algorithm SHA256).Hash -ne $rows[0].Groups[1].Value) { throw "Checksum mismatch for $item" }
    }
    $candidate = Join-Path $stage $name
    if ((& $candidate --version) -ne "blue-dragon $($tag.Substring(1))" -or $LASTEXITCODE -ne 0) { throw 'Version verification failed' }
    $installed = Join-Path $dir 'blue-dragon.exe'
    Copy-Item (Join-Path $stage 'LICENSES.txt') (Join-Path $dir 'LICENSES.txt') -Force
    if (Test-Path -LiteralPath $installed) {
        [IO.File]::Replace($candidate, $installed, (Join-Path $stage 'previous.exe'))
    } else {
        [IO.File]::Move($candidate, $installed)
    }
} finally {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
& $installed --version
exit $LASTEXITCODE
