$ErrorActionPreference = "Stop"
$Repo = "khdertaysh1/khdertaysh1-blue-dragon-agent"
$InstallDir = Join-Path $env:LOCALAPPDATA "blue-dragon\bin"

$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "blue-dragon-installer" }
$Asset = $Release.assets | Where-Object { $_.name -match "(?i)windows.*\.zip$" } | Select-Object -First 1
if (-not $Asset) { throw "No Windows .zip asset found in release $($Release.tag_name)." }

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("blue-dragon-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null
$ZipPath = Join-Path $TmpDir $Asset.name

try {
    Write-Host "Downloading $($Asset.name) ($($Release.tag_name))..."
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ZipPath

    $SumAsset = $Release.assets | Where-Object { $_.name -match "(?i)(sha256|checksums)" } | Select-Object -First 1
    if ($SumAsset) {
        $SumPath = Join-Path $TmpDir $SumAsset.name
        Invoke-WebRequest -Uri $SumAsset.browser_download_url -OutFile $SumPath
        $ExpectedLine = Get-Content $SumPath | Where-Object { $_ -match [regex]::Escape($Asset.name) } | Select-Object -First 1
        if ($ExpectedLine) {
            $Expected = ($ExpectedLine -split "\s+")[0].ToLower()
            $Actual = (Get-FileHash -Algorithm SHA256 -Path $ZipPath).Hash.ToLower()
            if ($Actual -ne $Expected) { throw "SHA256 mismatch for $($Asset.name)." }
            Write-Host "SHA256 verified."
        }
    }

    $ExtractDir = Join-Path $TmpDir "out"
    Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force
    $Exe = Get-ChildItem -Path $ExtractDir -Recurse -Filter "blue-dragon.exe" | Select-Object -First 1
    if (-not $Exe) { throw "blue-dragon.exe not found inside archive." }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $Exe.FullName -Destination (Join-Path $InstallDir "blue-dragon.exe") -Force

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $UserPath -or ($UserPath -split ";" | Where-Object { $_ -eq $InstallDir }).Count -eq 0) {
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir".Trim(";"), "User")
        $env:Path = "$env:Path;$InstallDir"
    }

    Write-Host "Installed Blue Dragon $($Release.tag_name) to $InstallDir"
    Write-Host "Open a new terminal and run: blue-dragon"
} finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
