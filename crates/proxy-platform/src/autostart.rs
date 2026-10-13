use anyhow::{Context, Result, bail};
use proxy_core::model::AutostartState;
use std::{os::windows::process::CommandExt, path::Path, process::Command};

const PRELUDE: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$name = 'RS Proxy ' + $identity.User.Value
$existing = Get-ScheduledTask | Where-Object { $_.TaskName -eq $name -and $_.TaskPath -eq '\' }
"#;

fn execute(script: &str, executable: Option<&Path>, minimized: bool) -> Result<String> {
    let system_root = std::env::var_os("SystemRoot").context("缺少 SystemRoot")?;
    let powershell = Path::new(&system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = Command::new(powershell);
    command.creation_flags(0x08000000).args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!("{PRELUDE}\n{script}"),
    ]);
    if let Some(path) = executable {
        command.env("RS_PROXY_EXECUTABLE", path);
    }
    command.env("RS_PROXY_MINIMIZED", if minimized { "1" } else { "0" });
    let output = command.output().context("运行 Windows 计划任务管理")?;
    if !output.status.success() {
        bail!(
            "计划任务操作失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("计划任务返回了无效的 UTF-8")
}

pub fn query() -> Result<AutostartState> {
    let json = execute(
        r#"
@{ enabled = [bool]$existing; start_minimized = [bool]($existing -and $existing.Actions.Arguments -eq '--minimized') } | ConvertTo-Json -Compress
"#,
        None,
        false,
    )?;
    serde_json::from_str(json.trim()).context("读取启动任务状态")
}

pub fn set(executable: &Path, enabled: bool, start_minimized: bool) -> Result<AutostartState> {
    let script = if enabled {
        r#"
if (-not (Test-Path -LiteralPath $env:RS_PROXY_EXECUTABLE -PathType Leaf)) { throw '找不到应用程序' }
$actionArgs = @{ Execute = $env:RS_PROXY_EXECUTABLE }
if ($env:RS_PROXY_MINIMIZED -eq '1') { $actionArgs.Argument = '--minimized' }
$action = New-ScheduledTaskAction @actionArgs
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
$principal = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero)
Register-ScheduledTask -TaskName $name -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Description 'RS Proxy 桌面代理' -Force | Out-Null
"#
    } else {
        r#"
if ($existing) { $existing | Unregister-ScheduledTask -Confirm:$false }
"#
    };
    execute(script, Some(executable), start_minimized)?;
    query()
}
