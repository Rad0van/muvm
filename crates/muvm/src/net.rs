use std::collections::HashMap;
use std::env;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use log::debug;
use rustix::io::dup;

use crate::utils::env::find_in_path;
use crate::utils::stdio::make_stdout_stderr;

struct PublishSpec<'a> {
    udp: bool,
    guest_range: (u32, u32),
    host_range: (u32, u32),
    ip: &'a str,
}

fn parse_range(r: &str) -> Result<(u32, u32)> {
    Ok(if let Some(pos) = r.find('-') {
        (r[..pos].parse()?, r[pos + 1..].parse()?)
    } else {
        let val = r.parse()?;
        (val, val)
    })
}

impl PublishSpec<'_> {
    fn parse(mut arg: &str) -> Result<PublishSpec<'_>> {
        let mut udp = false;
        if arg.ends_with("/udp") {
            udp = true;
        }
        if let Some(pos) = arg.rfind('/') {
            arg = &arg[..pos];
        }
        let guest_range_start = arg.rfind(':');
        let guest_range = parse_range(&arg[guest_range_start.map(|x| x + 1).unwrap_or(0)..])?;
        let mut ip = "";
        let host_range = match guest_range_start {
            None => guest_range,
            Some(guest_range_start) => {
                arg = &arg[..guest_range_start];
                let ip_start = arg.rfind(':');
                if let Some(ip_start) = ip_start {
                    ip = &arg[..ip_start];
                    arg = &arg[ip_start + 1..];
                }
                if arg.is_empty() {
                    guest_range
                } else {
                    parse_range(arg)?
                }
            },
        };
        Ok(PublishSpec {
            ip,
            host_range,
            guest_range,
            udp,
        })
    }
    fn to_args(&self) -> [String; 2] {
        // Bind to a specific IPv4 address (default 0.0.0.0) so passt creates an
        // IPv4-only listener instead of a dual-stack one. That frees the IPv6
        // loopback (::1) for muvm's own host-side proxy (browsers resolve
        // `localhost` to ::1 first, which passt accepts but never forwards).
        // See `setup_host_loopback6_proxies`.
        let ip = if self.ip.is_empty() { "0.0.0.0" } else { self.ip };
        [
            if self.udp { "-u" } else { "-t" }.to_owned(),
            format!(
                "{}/{}-{}:{}-{}",
                ip,
                self.host_range.0,
                self.host_range.1,
                self.guest_range.0,
                self.guest_range.1
            ),
        ]
    }
}

pub fn connect_to_passt<P>(passt_socket_path: P) -> Result<UnixStream>
where
    P: AsRef<Path>,
{
    Ok(UnixStream::connect(passt_socket_path)?)
}

pub fn start_passt(publish_ports: &[String]) -> Result<UnixStream> {
    // SAFETY: The child process should not inherit the file descriptor of
    // `parent_socket`. There is no documented guarantee of this, but the
    // implementation as of writing atomically sets `SOCK_CLOEXEC`.
    // See https://github.com/rust-lang/rust/blob/1.77.0/library/std/src/sys/pal/unix/net.rs#L124-L125
    // See https://github.com/rust-lang/rust/issues/47946#issuecomment-364776373
    let (parent_socket, child_socket) =
        UnixStream::pair().context("Failed to create socket pair for `passt` child process")?;

    // SAFETY: The parent process should not keep the file descriptor of
    // `child_socket` open. It is a `UnixStream` so the file descriptor will be
    // closed on drop.
    // See https://doc.rust-lang.org/std/io/index.html#io-safety
    //
    // The `dup` call clears the `FD_CLOEXEC` flag on the new `child_fd`, which
    // should be inherited by the child process.
    let child_fd = dup(child_socket)
        .context("Failed to duplicate file descriptor for `passt` child process")?;

    debug!(fd = child_fd.as_raw_fd(); "passing fd to passt");

    let mut cmd = Command::new("passt");
    // SAFETY: `child_fd` is an `OwnedFd` and consumed to prevent closing on drop,
    // as it will now be owned by the child process.
    // See https://doc.rust-lang.org/std/io/index.html#io-safety
    cmd.args(["-q", "-f", "--fd"])
        .arg(format!("{}", child_fd.into_raw_fd()));
    for spec in publish_ports {
        cmd.args(PublishSpec::parse(spec)?.to_args());
    }
    let child = cmd.spawn();
    if let Err(err) = child {
        return Err(err).context("Failed to execute `passt` as child process");
    }

    Ok(parent_socket)
}

/// Host TCP ports published *without* an explicit bind address. passt binds
/// these IPv4-only (see `PublishSpec::to_args`), so they each get an IPv6
/// loopback proxy below.
pub fn host_loopback_ports(publish_ports: &[String]) -> Result<Vec<u32>> {
    let mut ports = Vec::new();
    for spec in publish_ports {
        let spec = PublishSpec::parse(spec)?;
        if spec.udp || !spec.ip.is_empty() {
            continue;
        }
        for port in spec.host_range.0..=spec.host_range.1 {
            ports.push(port);
        }
    }
    Ok(ports)
}

/// For each host port, start a `socat` proxy listening on `[::1]:port` and
/// forwarding to `127.0.0.1:port` (where passt listens on IPv4). Without this,
/// connections to `localhost` that resolve to the IPv6 loopback first (browsers
/// do) never reach the guest, since passt is bound IPv4-only and its dual-stack
/// listener wouldn't forward IPv6 loopback anyway.
pub fn setup_host_loopback6_proxies(host_ports: &[u32]) -> Result<()> {
    if host_ports.is_empty() {
        return Ok(());
    }
    let socat_path = find_in_path("socat").context("Failed to check existence of `socat`")?;
    let Some(socat_path) = socat_path else {
        eprintln!("socat not found; IPv6 loopback proxies for published ports will not be set up");
        return Ok(());
    };
    let envs: HashMap<String, String> = env::vars().collect();
    for &port in host_ports {
        let (stdout, stderr) = make_stdout_stderr(&socat_path, &envs)?;
        Command::new(&socat_path)
            .arg(format!("TCP6-LISTEN:{port},bind=[::1],fork,reuseaddr"))
            .arg(format!("TCP4:127.0.0.1:{port}"))
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .with_context(|| format!("Failed to start IPv6 loopback proxy for port {port}"))?;
    }
    Ok(())
}
