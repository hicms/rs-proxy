param([string]$Archive)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $projectRoot 'src-tauri\resources\windivert'
$localArchive = $PSBoundParameters.ContainsKey('Archive')
if (-not $localArchive) { $Archive = Join-Path ([IO.Path]::GetTempPath()) 'rs-proxy-WinDivert-2.2.2-A.zip' }
$expectedHash = '63CB41763BB4B20F600B6DE04E991A9C2BE73279E317D4D82F237B150C5F3F15'
if (-not $localArchive -and (-not (Test-Path -LiteralPath $Archive) -or (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash -ne $expectedHash)) {
    Invoke-WebRequest 'https://github.com/basil00/WinDivert/releases/download/v2.2.2/WinDivert-2.2.2-A.zip' -OutFile $Archive
}
if ((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'WinDivert archive checksum mismatch.'
}
$resources = [IO.Path]::GetFullPath((Split-Path -Parent $destination))
$work = Join-Path $resources ('.windivert-prepare-' + [Guid]::NewGuid().ToString('N'))
$stage = Join-Path $work 'new'
$backup = Join-Path $work 'previous'
# All directory moves and cleanup stay inside this invocation's resource directory.
foreach ($path in @($destination, $work, $stage, $backup)) {
    if (-not [IO.Path]::GetFullPath($path).StartsWith($resources + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Resource preparation path is outside the resource directory.'
    }
}
$preserveBackup = $false
try {
    $extracted = Join-Path $work 'archive'
    Expand-Archive -LiteralPath $Archive -DestinationPath $extracted
    $distribution = Join-Path $extracted 'WinDivert-2.2.2-A'
    $driver = Join-Path $distribution 'x64\WinDivert64.sys'
    if ((Get-AuthenticodeSignature -LiteralPath $driver).Status -ne 'Valid') {
        throw 'The WinDivert driver signature is not valid.'
    }
    New-Item -ItemType Directory -Path $stage | Out-Null
    Copy-Item -LiteralPath $driver -Destination $stage
    Copy-Item -LiteralPath (Join-Path $distribution 'x64\WinDivert.dll') -Destination $stage
    Copy-Item -LiteralPath (Join-Path $distribution 'LICENSE') -Destination $stage
    $changed = @('WinDivert.dll', 'WinDivert64.sys', 'LICENSE') | Where-Object {
        $current = Join-Path $destination $_
        -not (Test-Path -LiteralPath $current -PathType Leaf) -or
            (Get-FileHash -LiteralPath $current).Hash -ne (Get-FileHash -LiteralPath (Join-Path $stage $_)).Hash
    }
    if ($changed) {
        if (Test-Path -LiteralPath $destination) { Move-Item -LiteralPath $destination -Destination $backup }
        try { Move-Item -LiteralPath $stage -Destination $destination } catch {
            $publishError = $_
            if (Test-Path -LiteralPath $backup) {
                try { Move-Item -LiteralPath $backup -Destination $destination } catch {
                    $preserveBackup = $true
                    throw "Resource publication and recovery failed; previous resources retained at ${backup}. Publication: $publishError Recovery: $_"
                }
            }
            throw $publishError
        }
    }
} finally {
    if (-not $preserveBackup -and (Test-Path -LiteralPath $work)) {
        Remove-Item -LiteralPath $work -Recurse -Force
    }
}
Write-Output 'WinDivert 2.2.2 x64 resources verified and ready.'
