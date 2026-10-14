use anyhow::{Result, bail};
use std::io;

pub(super) fn open(source: io::Error) -> anyhow::Error {
    let reason = match source.raw_os_error() {
        Some(2) => "未找到 WinDivert 驱动文件，请检查随程序提供的驱动资源",
        Some(5) => "没有访问驱动的权限，请以管理员身份运行 RS Proxy",
        Some(87) => "过滤表达式、抓包层、优先级或标志无效",
        Some(577) => "Windows 无法验证驱动数字签名",
        Some(654) => "系统仍加载着不兼容的 WinDivert 驱动",
        Some(1060) => "驱动服务未安装，当前打开方式禁止自动安装",
        Some(1275) => "驱动被系统阻止，请检查安全软件或虚拟化环境的驱动限制",
        Some(1753) => "Windows Base Filtering Engine 服务不可用",
        _ => "系统调用失败，请查看原始系统错误",
    };
    anyhow::Error::new(source).context(format!("打开 WinDivert 失败：{reason}"))
}

pub(super) fn require_supported_version(major: u64, minor: u64) -> Result<()> {
    if (major, minor) != (2, 2) {
        bail!("不支持的 WinDivert 驱动 API 版本 {major}.{minor}，需要 2.2 接口");
    }
    Ok(())
}
