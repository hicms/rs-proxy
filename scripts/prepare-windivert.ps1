$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $projectRoot 'src-tauri\resources\windivert'
$archive = Join-Path ([IO.Path]::GetTempPath()) 'rs-proxy-WinDivert-2.2.2-A.zip'
$expectedHash = '63CB41763BB4B20F600B6DE04E991A9C2BE73279E317D4D82F237B150C5F3F15'
if (-not (Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expectedHash) {
    Invoke-WebRequest 'https://github.com/basil00/WinDivert/releases/download/v2.2.2/WinDivert-2.2.2-A.zip' -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expectedHash) {
    throw 'WinDivert archive checksum mismatch.'
}
$extracted = Join-Path ([IO.Path]::GetTempPath()) 'rs-proxy-windivert'
Expand-Archive -LiteralPath $archive -DestinationPath $extracted -Force
$distribution = Join-Path $extracted 'WinDivert-2.2.2-A'
$driver = Join-Path $distribution 'x64\WinDivert64.sys'
if ((Get-AuthenticodeSignature -LiteralPath $driver).Status -ne 'Valid') {
    throw 'The WinDivert driver signature is not valid.'
}
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Copy-Item -LiteralPath $driver -Destination $destination
Copy-Item -LiteralPath (Join-Path $distribution 'x64\WinDivert.dll') -Destination $destination
Copy-Item -LiteralPath (Join-Path $distribution 'LICENSE') -Destination $destination
Write-Output 'WinDivert 2.2.2 x64 resources verified and ready.'
