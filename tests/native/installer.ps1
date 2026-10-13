$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (Get-Process rs-proxy -ErrorAction SilentlyContinue) { throw 'Close RS Proxy before installer verification.' }
$version = (Get-Content -LiteralPath (Join-Path $repo 'package.json') -Raw | ConvertFrom-Json).version
$installer = Join-Path $repo "target\release\bundle\nsis\RS Proxy_${version}_x64-setup.exe"
if (-not (Test-Path -LiteralPath $installer)) { throw 'Build the release NSIS installer first.' }
$manufacturer = 'HKCU:\Software\rsproxy\RS Proxy'
$manufacturerParentExisted = Test-Path -LiteralPath 'HKCU:\Software\rsproxy'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\RS Proxy'
foreach ($key in @($manufacturer, $uninstallKey, 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\RS Proxy', 'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\RS Proxy')) {
  if (Test-Path -LiteralPath $key) { throw "Existing registration; refusing to replace: $key" }
}
$shortcuts = @((Join-Path ([Environment]::GetFolderPath('Desktop')) 'RS Proxy.lnk'), (Join-Path ([Environment]::GetFolderPath('Programs')) 'RS Proxy.lnk'))
foreach ($shortcut in $shortcuts) { if (Test-Path -LiteralPath $shortcut) { throw "Existing shortcut; refusing to replace: $shortcut" } }
$runKey = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if ($runKey.PSObject.Properties.Name -contains 'RS Proxy') { throw 'Existing Run entry; refusing to replace.' }
$data = Join-Path $env:LOCALAPPDATA 'dev.rsproxy.desktop'
$configPath = Join-Path $data 'config.json'
if (Test-Path -LiteralPath (Join-Path $data 'dns-recovery.json')) { throw 'DNS recovery journal exists; refusing to launch.' }
$originalConfig = if (Test-Path -LiteralPath $configPath) { [IO.File]::ReadAllBytes($configPath) } else { $null }
if (-not $originalConfig) { throw 'An existing DNS-disabled configuration is required for this installed-app smoke test.' }
$config = [Text.Encoding]::UTF8.GetString($originalConfig) | ConvertFrom-Json
if ($config.dns.enabled) { throw 'DNS must be disabled; this test never changes it.' }
$config.ui.close_to_tray = $false
$dnsBefore = Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses | ConvertTo-Json -Depth 5 -Compress
$tempRoot = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\') + '\'
$installRoot = [IO.Path]::GetFullPath((Join-Path $tempRoot ('rs-proxy-install-' + [Guid]::NewGuid().ToString('N'))))
if (-not $installRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Installation target must stay within TEMP.' }
$installedExe = Join-Path $installRoot 'rs-proxy.exe'
$application = $null
$setup = $null
$failure = $null
$cleanupErrors = [Collections.Generic.List[string]]::new()
function Invoke-Cleanup([scriptblock]$Action) { try { & $Action } catch { $cleanupErrors.Add($_.ToString()) } }
try {
  $setup = Start-Process -FilePath $installer -ArgumentList @('/S', '/NS', "/D=$installRoot") -WindowStyle Hidden -PassThru
  if (-not $setup.WaitForExit(60000) -or $setup.ExitCode -ne 0) { throw 'Installer did not complete successfully.' }
  foreach ($file in @('rs-proxy.exe', 'windivert\WinDivert.dll', 'windivert\WinDivert64.sys', 'licenses\Lucide-LICENSE.txt', 'uninstall.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $installRoot $file))) { throw "Missing installed resource: $file" }
  }
  # Tauri patches the bundle marker for NSIS and then restores the standalone binary.
  # https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle.rs
  $expectedBinary = [IO.File]::ReadAllBytes((Join-Path $repo 'target\release\rs-proxy.exe'))
  $binaryText = [Text.Encoding]::ASCII.GetString($expectedBinary)
  $marker = '__TAURI_BUNDLE_TYPE_VAR_UNK'
  $markerIndex = $binaryText.IndexOf($marker, [StringComparison]::Ordinal)
  if ($markerIndex -lt 0 -or $markerIndex -ne $binaryText.LastIndexOf($marker, [StringComparison]::Ordinal)) { throw 'Expected one Tauri bundle marker.' }
  [Array]::Copy([Text.Encoding]::ASCII.GetBytes('__TAURI_BUNDLE_TYPE_VAR_NSS'), 0, $expectedBinary, $markerIndex, $marker.Length)
  $expectedHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($expectedBinary))
  if ((Get-FileHash -LiteralPath $installedExe).Hash -ne $expectedHash) { throw 'Installed binary differs beyond the documented NSIS bundle marker.' }
  if ((Get-ItemProperty -LiteralPath $uninstallKey).InstallLocation.Trim('"') -ne $installRoot) { throw 'Incorrect installation registration.' }
  $config | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $configPath -Encoding utf8
  foreach ($appExecutable in @($installedExe, (Join-Path $repo 'target\debug\rs-proxy.exe'))) {
  $application = Start-Process -FilePath $appExecutable -WindowStyle Hidden -PassThru
  for ($attempt = 0; $attempt -lt 100; $attempt++) {
    $application.Refresh()
    if ($application.HasExited) { throw "Installed application exited: $($application.ExitCode)" }
    if ($application.MainWindowTitle -eq 'RS Proxy' -and @($application.Modules | Where-Object { $_.ModuleName -eq 'WinDivert.dll' }).Count) { break }
    Start-Sleep -Milliseconds 200
  }
  if ($application.MainWindowTitle -ne 'RS Proxy') { throw 'Installed main window did not open.' }
  if (-not @($application.Modules | Where-Object { $_.ModuleName -eq 'WinDivert.dll' }).Count) { throw 'Installed application did not load its driver library.' }
  if (Get-NetTCPConnection -LocalPort 9225 -State Listen -ErrorAction SilentlyContinue) { throw 'Unexpected QA debugging port in release smoke test.' }
  & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -File (Join-Path $PSScriptRoot 'windows.ps1') -Action close -ProcessId $application.Id -Value 'RS Proxy'
  if ($LASTEXITCODE -ne 0 -or -not $application.WaitForExit(10000) -or $application.ExitCode -ne 0) { throw 'Installed application did not exit cleanly.' }
  }
} catch { $failure = $_ } finally {
  Invoke-Cleanup {
    if ($setup -and -not $setup.HasExited) { Stop-Process -Id $setup.Id; if (-not $setup.WaitForExit(10000)) { throw 'Installer did not stop.' } }
  }
  Invoke-Cleanup {
    if ($application -and -not $application.HasExited) { Stop-Process -Id $application.Id; if (-not $application.WaitForExit(10000)) { throw 'Installed test process did not stop.' } }
  }
  Invoke-Cleanup {
    if ($application -and -not $application.HasExited) { throw 'Process still running; refusing configuration restore.' }
    [IO.File]::WriteAllBytes($configPath, $originalConfig)
  }
  Invoke-Cleanup {
    if (($setup -and -not $setup.HasExited) -or ($application -and -not $application.HasExited)) { throw 'Owned process still running; refusing concurrent uninstall.' }
    $uninstaller = Join-Path $installRoot 'uninstall.exe'
    if (Test-Path -LiteralPath $uninstaller) {
      $removal = Start-Process -FilePath $uninstaller -ArgumentList @('/S', "_?=$installRoot") -WindowStyle Hidden -PassThru
      if (-not $removal.WaitForExit(60000)) { Stop-Process -Id $removal.Id; if (-not $removal.WaitForExit(10000)) { throw 'Uninstaller could not be stopped; registration cleanup skipped.' }; throw 'Uninstaller timed out and was stopped.' }
      if ($removal.ExitCode -ne 0) { throw 'Uninstaller did not complete successfully.' }
    }
    if ((Test-Path -LiteralPath $installedExe) -or (Test-Path -LiteralPath $uninstallKey)) { throw 'Installed application or registration remains.' }
    if (Test-Path -LiteralPath $manufacturer) {
      if ((Get-Item -LiteralPath $manufacturer).GetValue('') -ne $installRoot) { throw 'Manufacturer registration belongs to a different install.' }
      Remove-Item -LiteralPath $manufacturer
    }
    if (-not $manufacturerParentExisted -and (Test-Path -LiteralPath 'HKCU:\Software\rsproxy')) {
      $parentKey = Get-Item -LiteralPath 'HKCU:\Software\rsproxy'
      if ($parentKey.SubKeyCount -eq 0 -and $parentKey.ValueCount -eq 0) { Remove-Item -LiteralPath 'HKCU:\Software\rsproxy' }
    }
    if (Test-Path -LiteralPath $uninstaller) { Remove-Item -LiteralPath $uninstaller }
    if ((Test-Path -LiteralPath $installRoot) -and -not @(Get-ChildItem -LiteralPath $installRoot -Force).Count) { Remove-Item -LiteralPath $installRoot }
  }
  Invoke-Cleanup {
    foreach ($shortcut in $shortcuts) { if (Test-Path -LiteralPath $shortcut) { throw 'Unexpected shortcut remains.' } }
    if ((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run').PSObject.Properties.Name -contains 'RS Proxy') { throw 'Unexpected Run entry remains.' }
    $dnsAfter = Get-DnsClientServerAddress | Sort-Object InterfaceIndex,AddressFamily | Select-Object InterfaceIndex,AddressFamily,ServerAddresses | ConvertTo-Json -Depth 5 -Compress
    if ($dnsBefore -ne $dnsAfter -or (Test-Path -LiteralPath (Join-Path $data 'dns-recovery.json'))) { throw 'DNS baseline differs; no restoration writes are performed.' }
    if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($configPath)) -ne [Convert]::ToBase64String($originalConfig)) { throw 'User configuration differs from baseline.' }
  }
}
if ($cleanupErrors.Count) { throw "Test error: $failure; cleanup errors: $($cleanupErrors -join '; ')" }
if ($failure) { throw $failure }
@{ installed=$true; releaseStarted=$true; debugStarted=$true; cleanExit=$true; uninstalled=$true; configRestored=$true; dnsUnchanged=$true; installRoot=$installRoot; remainingFiles=@(Get-ChildItem -LiteralPath $installRoot -Recurse -File -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName) } | ConvertTo-Json | Set-Content (Join-Path $env:TEMP 'rs-proxy-e2e\installer.json')
Write-Output 'PASS installer, release startup/resources, clean exit, uninstall and baseline restoration'
