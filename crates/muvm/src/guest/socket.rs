use std::collections::HashMap;
use std::env;
use std::net::Ipv4Addr;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::utils::env::find_in_path;
use crate::utils::stdio::make_stdout_stderr;

/// For each (host_port, guest_port) pair, start a socat proxy that listens on
/// `eth0_ip:host_port` and forwards to `127.0.0.1:guest_port`.  This is needed
/// because passt delivers forwarded TCP connections to the guest's external
/// (eth0) address while many guest applications only listen on loopback.
pub fn setup_loopback_tcp_proxies(
    eth0_ip: Ipv4Addr,
    port_pairs: &[(u32, u32)],
) -> Result<()> {
    if port_pairs.is_empty() {
        return Ok(());
    }
    let socat_path = find_in_path("socat").context("Failed to check existence of `socat`")?;
    let Some(socat_path) = socat_path else {
        eprintln!("socat not found; loopback TCP proxies for published ports will not be set up");
        return Ok(());
    };
    let envs: HashMap<String, String> = env::vars().collect();
    for &(host_port, guest_port) in port_pairs {
        let (stdout, stderr) = make_stdout_stderr(&socat_path, &envs)?;
        Command::new(&socat_path)
            .arg(format!(
                "TCP-LISTEN:{host_port},bind={eth0_ip},fork,reuseaddr"
            ))
            .arg(format!("TCP:127.0.0.1:{guest_port}"))
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .with_context(|| {
                format!(
                    "Failed to start loopback TCP proxy for port {host_port} -> {guest_port}"
                )
            })?;
    }
    Ok(())
}

pub fn setup_socket_proxy<P>(socket_path: P, port: u32) -> Result<()>
where
    P: AsRef<Path>,
{
    let socat_path = find_in_path("socat").context("Failed to check existence of `socat`")?;
    let Some(socat_path) = socat_path else {
        return Ok(());
    };

    let envs: HashMap<String, String> = env::vars().collect();
    let (stdout, stderr) = make_stdout_stderr(&socat_path, &envs)?;

    Command::new(socat_path)
        .arg(format!(
            "UNIX-LISTEN:{},fork",
            socket_path
                .as_ref()
                .to_str()
                .expect("socket_path should not contain invalid UTF-8")
        ))
        .arg(format!("VSOCK-CONNECT:2:{port}"))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .context("Failed to execute `socat` as child process")?;

    Ok(())
}
