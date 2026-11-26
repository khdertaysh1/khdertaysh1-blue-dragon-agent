# Register an existing Blue Dragon binary, including a downloaded draft build.
param([Parameter(Mandatory = $true)][string]$Binary)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Binary).Path
$programs = [Environment]::GetFolderPath('Programs')
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $programs 'Blue Dragon.lnk'))
$shortcut.TargetPath = $exe
$shortcut.Arguments = 'gui'
$shortcut.WorkingDirectory = [Environment]::GetFolderPath('UserProfile')
$shortcut.WindowStyle = 7
$shortcut.Save()
