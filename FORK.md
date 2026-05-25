# About this fork

This is a fork of [AsahiLinux/muvm](https://github.com/AsahiLinux/muvm) carrying a
handful of patches that make muvm usable as the **x86-64 runtime for the Slovak
DITEC eID signing stack on Apple Silicon / Asahi Linux**.

Upstream muvm targets running x86 programs (mostly games) from the host system in a
4 KiB-page libkrun microVM with FEX emulation — needed on aarch64 Asahi because the
host kernel uses 16 KiB pages, which x86 binaries (jemalloc) can't tolerate. The
eID software (D.Launcher 2 / D.Bridge 2, eID Klient, D.Signer Java) is x86-64-only
and assumes an ordinary x86 Linux, so it runs inside muvm too — but it needs a few
things upstream muvm doesn't provide out of the box: a **smartcard (PC/SC) bridge**,
the **eID Klient localhost port reachable from the host browser over both IPv4 and
IPv6**, and a couple of **robustness fixes** for the FEX-emulated JVM and for being
launched from non-interactive contexts.

These patches are consumed by the **[eid-stack](https://github.com/Rad0van/eid-stack)**
installer, which builds this fork into `~/.cargo/bin/{muvm,muvm-guest}` and wires up
the rest of the stack (systemd unit, native-messaging wrappers, the FEX RootFS, …).

- **Branch:** `pcscd`, based on upstream `main` at `ea73c7c` ("Bump version to 0.5.1").
- **Build & install:** `cargo build --release`, then place `muvm` and `muvm-guest`
  on `PATH` (the host `muvm` finds `muvm-guest` via `$PATH` only, so both must be
  reachable). Replacing the host `muvm` while a persistent VM is running needs a
  rename (`mv`), not `cp` — the running file is "Text file busy" for `cp`.

## Patches (what & why)

Each runs in either the **host** `muvm` binary or the in-guest `muvm-guest` init.

### Smartcard / eID

- **`Add pcscd forwarding`** (`284c854`, host + guest) — forwards the host's pcscd
  socket into the guest over vsock and exports `PCSCLITE_CSOCK_NAME` there, so the
  USB smartcard reader (visible to host pcscd) is usable by eID Klient / D.Signer
  inside the VM. Without it the card session never opens.

- **`Add loopback TCP proxies for published ports`** (`721bd1e`, guest) — for each
  `--publish`ed port, the guest starts `socat eth0_ip:port → 127.0.0.1:port`. passt
  delivers forwarded connections to the guest's **eth0** address, but eID Klient
  binds only the guest **loopback** (`127.0.0.1:15480`). This bridges the two so the
  host browser's tcToken request reaches eID Klient.

- **`net: forward IPv6 localhost for published ports`** (`a7990bf`, host) — browsers
  resolve `localhost` to `::1` first, but passt binds published ports dual-stack and
  accepts `::1` **without forwarding it**, so `localhost:<port>` fails over IPv6
  (the portal reports "eID klient not running"). This binds passt **IPv4-only** for
  ports published without an explicit address and starts a host-side
  `socat [::1]:port → 127.0.0.1:port` per port (`setup_host_loopback6_proxies`,
  mirroring the guest-side proxy above). Now `localhost:<port>` works over both
  families. This one is generic (not eID-specific) and is the most upstreamable.

### Robustness

- **`muvm: tolerate non-pollable stdin in run_io_host`** (`23ae129`, host) — treats
  `epoll_ctl(stdin, EPOLLIN)` returning `EPERM` as immediate EOF instead of bailing,
  so muvm can be launched with a regular-file / `/dev/null` / non-pollable stdin
  (e.g. from `.desktop` launchers, or a browser native-messaging host) without
  failing with `could not request launch to server: EPERM`.

- **`muvm-guest: set vm.overcommit_memory=1 for FEX fork()`** (`d3c232b`, guest) —
  FEX-emulated processes reserve tens of GB of virtual/anon memory (the eID JVM:
  `VmSize` ~131 TB, `VmData` ~19 GB). With the guest's default heuristic overcommit
  and a small `CommitLimit` (no swap, 50% of `--mem`), `fork()`/`posix_spawn` from
  such a process is refused with `ENOMEM` even though RAM is free — which broke eID
  Klient spawning its on-screen keyboard for contactless PIN entry. Setting
  always-overcommit at guest init lets these forks succeed.

- **`muvm-guest: use create_dir_all for pulse XDG path`** (`fe1da9e`, guest) —
  defensive: a stale/partly-present pulse dir no longer aborts guest setup before the
  pcscd bridge is configured.

## Relationship to upstream

The networking and robustness fixes are intended to be generally useful; the IPv6
and stdin patches in particular are clean candidates for upstreaming. The pcscd
bridge is more eID-specific. None of these change muvm's default behavior for the
common case beyond binding published ports IPv4-only (IPv6 *loopback* is handled by
the new proxy; IPv6 over passt to the guest never worked anyway).
