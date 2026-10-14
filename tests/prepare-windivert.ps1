param([Parameter(Mandatory)][string]$Archive)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('rs-proxy-resource-test-' + [Guid]::NewGuid().ToString('N'))
$fixture = [IO.Path]::GetFullPath($fixture)
if ((Split-Path -Parent $fixture) -ne [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')) { throw 'Invalid fixture path.' }
$script = Join-Path $fixture 'scripts/prepare-windivert.ps1'
$resources = Join-Path $fixture 'src-tauri/resources'
$destination = Join-Path $resources 'windivert'
function Snapshot {
    Get-ChildItem -LiteralPath $destination -File | Sort-Object Name | ForEach-Object {
        "$($_.Name):$((Get-FileHash -LiteralPath $_.FullName).Hash):$($_.LastWriteTimeUtc.Ticks)"
    } | ConvertTo-Json -Compress
}
try {
    New-Item -ItemType Directory -Path (Split-Path -Parent $script) -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repo 'scripts/prepare-windivert.ps1') -Destination $script
    & $script -Archive $Archive
    $before = Snapshot
    & $script -Archive $Archive
    if ((Snapshot) -ne $before) { throw 'Identical resources were rewritten.' }
    $badArchive = Join-Path $fixture 'invalid.zip'
    Set-Content -LiteralPath $badArchive -Value 'not a release'
    $rejected = $false
    try { & $script -Archive $badArchive } catch {
        if ($_.Exception.Message -notmatch 'checksum mismatch') { throw }
        $rejected = $true
    }
    if (-not $rejected -or (Snapshot) -ne $before) { throw 'Invalid archive changed resources or was accepted.' }

    # Inject a publication failure at the filesystem boundary, after the old group moves.
    Set-Content -LiteralPath (Join-Path $destination 'WinDivert.dll') -Value 'previous fixture'
    $before = Snapshot
    function Move-Item {
        param([string]$LiteralPath, [string]$Destination)
        if ((Split-Path -Leaf $LiteralPath) -eq 'new') { throw 'Injected publication failure' }
        Microsoft.PowerShell.Management\Move-Item -LiteralPath $LiteralPath -Destination $Destination
    }
    $rejected = $false
    try { & $script -Archive $Archive } catch {
        if ($_.Exception.Message -notmatch 'Injected publication failure') { throw }
        $rejected = $true
    } finally { Remove-Item Function:\Move-Item }
    if (-not $rejected -or (Snapshot) -ne $before) { throw 'Publication failure did not restore the previous group.' }

    function Move-Item {
        param([string]$LiteralPath, [string]$Destination)
        if ((Split-Path -Leaf $LiteralPath) -in @('new', 'previous')) { throw 'Injected move failure' }
        Microsoft.PowerShell.Management\Move-Item -LiteralPath $LiteralPath -Destination $Destination
    }
    $rejected = $false
    try { & $script -Archive $Archive } catch {
        if ($_.Exception.Message -notmatch 'previous resources retained at') { throw }
        $rejected = $true
    } finally { Remove-Item Function:\Move-Item }
    $backups = @(Get-ChildItem -LiteralPath $resources -Directory -Filter '.windivert-prepare-*')
    if (-not $rejected -or $backups.Count -ne 1) { throw 'Failed recovery did not retain a backup.' }
    $destination = Join-Path $backups[0].FullName 'previous'
    if ((Snapshot) -ne $before) { throw 'Retained backup differs from the previous group.' }
    Write-Output 'PASS resource preparation: official archive, no-op repeat, checksum rejection, publication rollback, retained recovery backup.'
} finally {
    if (Test-Path -LiteralPath $fixture) { Remove-Item -LiteralPath $fixture -Recurse -Force }
}
