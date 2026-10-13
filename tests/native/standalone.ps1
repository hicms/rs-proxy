param([string]$Executable = 'target\debug\rs-proxy.exe')
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location -LiteralPath $repo
if (Get-Process -Name rs-proxy -ErrorAction SilentlyContinue) { throw 'Close RS Proxy before standalone startup verification.' }
if (Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue) { throw 'Port 1420 must be unused; this test verifies startup without a dev server.' }
$appExe = (Resolve-Path -LiteralPath $Executable).Path
$data = Join-Path $env:LOCALAPPDATA 'dev.rsproxy.desktop'
$configPath = Join-Path $data 'config.json'
if (Test-Path -LiteralPath (Join-Path $data 'dns-recovery.json')) { throw 'DNS recovery journal exists; refusing to launch.' }
$originalConfig = [IO.File]::ReadAllBytes($configPath)
$config = [Text.Encoding]::UTF8.GetString($originalConfig) | ConvertFrom-Json
if ($config.dns.enabled) { throw 'DNS must already be disabled; this test never changes it.' }
$configHash = (Get-FileHash -LiteralPath $configPath).Hash
$binaryHash = (Get-FileHash -LiteralPath $appExe).Hash
$dnsBefore = Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses | ConvertTo-Json -Depth 5 -Compress
$application = $null
$failure = $null
try {
  $config.ui.close_to_tray = $false
  $config | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $configPath -Encoding utf8
  $application = Start-Process -FilePath $appExe -WindowStyle Hidden -PassThru
  $windows = Join-Path $PSScriptRoot 'windows.ps1'
  $windowsPowerShell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  $controls = @('进程与连接', '重连已选代理', '暂停代理') | ConvertTo-Json -Compress
  & $windowsPowerShell -NoProfile -File $windows -Action page -ProcessId $application.Id -Value $controls
  if ($LASTEXITCODE -ne 0) { throw 'The exact desktop executable failed standalone page verification.' }
  & $windowsPowerShell -NoProfile -File $windows -Action close -ProcessId $application.Id -Value 'RS Proxy'
  if ($LASTEXITCODE -ne 0 -or -not $application.WaitForExit(10000) -or $application.ExitCode -ne 0) { throw 'Standalone test application did not exit cleanly.' }
} catch { $failure = $_ } finally {
  if ($application -and -not $application.HasExited) {
    Stop-Process -Id $application.Id
    if (-not $application.WaitForExit(10000)) { throw 'Owned test process could not be stopped.' }
  }
  [IO.File]::WriteAllBytes($configPath, $originalConfig)
  $dnsAfter = Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses | ConvertTo-Json -Depth 5 -Compress
  if ($dnsBefore -ne $dnsAfter -or (Test-Path -LiteralPath (Join-Path $data 'dns-recovery.json'))) { throw 'DNS baseline differs; no restoration writes are performed.' }
  if ($configHash -ne (Get-FileHash -LiteralPath $configPath).Hash) { throw 'User configuration changed.' }
  if ($binaryHash -ne (Get-FileHash -LiteralPath $appExe).Hash) { throw 'Desktop executable changed during verification.' }
}
if ($failure) { throw $failure }
Write-Output "PASS exact executable startup without a development server: $appExe; SHA256 $binaryHash; config and DNS unchanged"
