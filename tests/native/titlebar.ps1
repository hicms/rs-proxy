#Requires -RunAsAdministrator
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location -LiteralPath $repo
if (Get-Process -Name rs-proxy -ErrorAction SilentlyContinue) { throw 'Close RS Proxy before title bar verification.' }
if (Get-NetTCPConnection -LocalPort 1420,9226 -State Listen -ErrorAction SilentlyContinue) { throw 'Ports 1420 and 9226 must be unused.' }
$standalone = Join-Path $repo 'target\debug\rs-proxy.exe'
$standaloneHash = (Get-FileHash -LiteralPath $standalone).Hash
$previousTarget = $env:CARGO_TARGET_DIR
$testTarget = Join-Path $repo 'target\native-e2e'
$override = Join-Path $env:TEMP ('rs-proxy-caption-' + [Guid]::NewGuid().ToString('N') + '.json')
$windows = (Get-Content -LiteralPath 'src-tauri\tauri.conf.json' -Raw | ConvertFrom-Json).app.windows
$windows[0] | Add-Member -MemberType NoteProperty -Name additionalBrowserArgs -Value '--remote-debugging-port=9226 --remote-debugging-address=127.0.0.1'
@{ app=@{ windows=$windows } } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $override -Encoding utf8
try {
  $env:CARGO_TARGET_DIR = $testTarget
  & npm run desktop:build -- --debug --config $override
  if ($LASTEXITCODE -ne 0) { throw 'Title bar test build failed.' }
  & (Join-Path $PSScriptRoot 'standalone.ps1') -Executable (Join-Path $testTarget 'debug\rs-proxy.exe') -Titlebar
} finally {
  $env:CARGO_TARGET_DIR = $previousTarget
  Remove-Item -LiteralPath $override
  if ((Get-FileHash -LiteralPath $standalone).Hash -ne $standaloneHash) { throw 'Title bar verification changed the standalone executable.' }
}
