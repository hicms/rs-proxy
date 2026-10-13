$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location -LiteralPath $repo
if (@(Get-Process -Name 'rs-proxy' -ErrorAction SilentlyContinue).Count) { throw 'Close RS Proxy before native tests; two capture engines must not run together.' }
$evidence = Join-Path $env:TEMP 'rs-proxy-e2e'
New-Item -ItemType Directory -Path $evidence -Force | Out-Null
$data = Join-Path $env:LOCALAPPDATA 'dev.rsproxy.e2e'
$configPath = Join-Path $data 'config.json'
if (Test-Path -LiteralPath (Join-Path $data 'dns-recovery.json')) { throw 'DNS recovery journal exists; refusing to start.' }
$originalConfig = if (Test-Path -LiteralPath $configPath) { [IO.File]::ReadAllBytes($configPath) } else { $null }
if ($originalConfig -and ([Text.Encoding]::UTF8.GetString($originalConfig) | ConvertFrom-Json).dns.enabled) { throw 'DNS must be disabled for native tests.' }
$dnsBefore = Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses
$dnsBefore | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $evidence 'dns-before.json')
$taskName = 'RS Proxy ' + [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$oldTask = Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue
$oldTaskXml = if ($oldTask) { Export-ScheduledTask -TaskName $taskName -TaskPath '\' } else { $null }
$override = Join-Path $evidence 'tauri-test.json'
@{ identifier='dev.rsproxy.e2e'; app=@{ windows=@(@{ label='main'; title='RS Proxy E2E'; width=1280; height=820; minWidth=760; minHeight=560; additionalBrowserArgs='--remote-debugging-port=9225 --remote-debugging-address=127.0.0.1' }) } } | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $override
$oldTarget = $env:CARGO_TARGET_DIR
$testTarget = Join-Path $repo 'target\native-e2e'
$executable = Join-Path $testTarget 'debug\rs-proxy.exe'
$application = $null
$failure = $null
$cleanupErrors = [Collections.Generic.List[string]]::new()
function Invoke-Cleanup([scriptblock]$Action) { try { & $Action } catch { $cleanupErrors.Add($_.ToString()) } }
try {
  # Always build this configuration into its own target; never reuse the normal desktop executable.
  $env:CARGO_TARGET_DIR = $testTarget
  & npm run desktop:build -- --debug --config $override
  if ($LASTEXITCODE -ne 0) { throw 'Test build failed' }
  $buildHash = (Get-FileHash -LiteralPath $executable).Hash
  if (Test-Path -LiteralPath $configPath) { Remove-Item -LiteralPath $configPath }
  $env:RS_PROXY_E2E_EXE = $executable
  $application = Start-Process -FilePath $executable -WindowStyle Hidden -PassThru
  $env:RS_PROXY_E2E_PID = [string]$application.Id
  & node tests/native/desktop.mjs
  if ($LASTEXITCODE -ne 0) { throw 'Native end-to-end tests failed' }
  if (-not $application.WaitForExit(10000)) { throw 'Application did not exit after first phase' }
  if ((Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json).dns.enabled -or (Test-Path -LiteralPath (Join-Path $data 'dns-recovery.json'))) { throw 'Unsafe DNS state before restart' }
  if ((Get-FileHash -LiteralPath $executable).Hash -ne $buildHash) { throw 'QA executable changed before restart' }
  $application = Start-Process -FilePath $executable -ArgumentList '--minimized' -WindowStyle Hidden -PassThru
  $env:RS_PROXY_E2E_PID = [string]$application.Id
  & node tests/native/restart.mjs
  if ($LASTEXITCODE -ne 0) { throw 'Restart end-to-end tests failed' }
} catch { $failure = $_ } finally {
  Invoke-Cleanup {
    if ($application -and -not $application.HasExited) {
      & node tests/native/quit.mjs
      if (-not $application.WaitForExit(10000)) {
        Stop-Process -Id $application.Id
        if (-not $application.WaitForExit(10000)) { throw 'Owned application did not stop; configuration not restored' }
      }
    }
  }
  Invoke-Cleanup {
    if ($oldTaskXml) { Register-ScheduledTask -TaskName $taskName -TaskPath '\' -Xml $oldTaskXml -Force | Out-Null }
    else { Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue | Unregister-ScheduledTask -Confirm:$false }
  }
  Invoke-Cleanup {
    if ($application -and -not $application.HasExited) { throw 'Application still running; refusing to overwrite its configuration' }
    if ($null -ne $originalConfig) { [IO.File]::WriteAllBytes($configPath, $originalConfig) }
    elseif (Test-Path -LiteralPath $configPath) { Remove-Item -LiteralPath $configPath }
  }
  Invoke-Cleanup {
    $dnsAfter = Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses
    $dnsAfter | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $evidence 'dns-after.json')
    if (($dnsBefore | ConvertTo-Json -Depth 5 -Compress) -ne ($dnsAfter | ConvertTo-Json -Depth 5 -Compress)) { throw 'DNS settings changed; inspect snapshots. This harness never writes system DNS.' }
    if (Test-Path -LiteralPath (Join-Path $data 'dns-recovery.json')) { throw 'Unexpected DNS journal exists.' }
  }
  Invoke-Cleanup {
    if ($oldTaskXml) { if ((Export-ScheduledTask -TaskName $taskName -TaskPath '\') -ne $oldTaskXml) { throw 'Autostart task restoration differs from baseline.' } }
    elseif (Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue) { throw 'Temporary autostart task remains.' }
  }
  $env:CARGO_TARGET_DIR = $oldTarget
  Remove-Item Env:RS_PROXY_E2E_PID,Env:RS_PROXY_E2E_EXE -ErrorAction SilentlyContinue
}
if ($cleanupErrors.Count) { throw "Test error: $failure; cleanup errors: $($cleanupErrors -join '; ')" }
if ($failure) { throw $failure }
