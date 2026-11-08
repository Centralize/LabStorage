# Installing iSCSI Manager

## Requirements

| What | Why |
|---|---|
| Ubuntu Desktop 26.04 or newer (GNOME, Wayland or X11) | The app is built and tested there |
| **open-iscsi** package | Provides `iscsiadm` and the `iscsid` service the app controls |
| polkit (`pkexec`) | Shows the password prompt for administrator actions. Installed by default on Ubuntu Desktop |
| An account that can administer the computer (member of the `sudo` group) | Connecting, disconnecting and changing settings need administrator rights |
| Optional: **Disks** (`gnome-disk-utility`) | Used by the *Open in Disks* button. Installed by default on Ubuntu Desktop |

To build from source you also need the Rust toolchain (version 1.95 or newer).

## 1. Install open-iscsi

Ubuntu Desktop doesn't include open-iscsi. Install it from a terminal:

```bash
sudo apt update
sudo apt install open-iscsi
sudo systemctl enable --now iscsid.service
```

You can also skip this step: when open-iscsi is missing, the app starts in sample
mode and has an **Install open-iscsi** button that runs the same commands after
asking for your password.

## 2. Get the Rust toolchain (to build from source)

If `cargo --version` already prints a version, skip this step.

```bash
sudo apt install build-essential curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

## 3. Build and install

From the folder that contains this app (the one with `Cargo.toml` and `install.sh`):

```bash
./install.sh
```

This builds an optimized version (the first build takes a few minutes) and installs:

| File | Installed to |
|---|---|
| The program | `~/.local/bin/iscsi-manager` |
| App-grid launcher | `~/.local/share/applications/iscsi-manager.desktop` |
| Icon | `~/.local/share/icons/hicolor/scalable/apps/iscsi-manager.svg` |

**iSCSI Manager** then appears in the app grid. You may need to log out and back in
before the icon shows up.

### Install for every user on the computer

```bash
./install.sh --system
```

This installs to `/usr/local` instead and asks for your password through `sudo`.

### Other options

| Command | What it does |
|---|---|
| `./install.sh --prefix /some/folder` | Install under another folder |
| `./install.sh --no-build` | Install the program that was already built, without building again |
| `./install.sh --uninstall` | Remove what the script installed (add `--system` or `--prefix` if you used them) |

### Build only, without installing

```bash
cargo build --release
./target/release/iscsi-manager
```

The program is a single file that only needs the system's standard libraries. You
can copy `target/release/iscsi-manager` to another Ubuntu 26.04 computer and run it
there.

## 4. First start

1. Open **iSCSI Manager** from the app grid.
2. Go to **This computer** and check that the *iscsid* service shows **Running**. If
   it doesn't, press **Start** and tick **Start at boot**.
3. Copy the **iSCSI name** shown there and add it to the allowed initiators (also
   called hosts or access list) on your storage server.
4. Continue with the [user guide](USER_GUIDE.md).

## Updating

Get the new source, then run `./install.sh` again. It replaces the installed program.

## Removing

```bash
./install.sh --uninstall
```

This removes only the app. Your saved targets and iSCSI settings belong to
open-iscsi and stay in place. To remove them as well, disconnect everything in the
app first, then run:

```bash
sudo apt purge open-iscsi
```

## Running without installing (sample mode)

To look around without any storage server, and without changing anything:

```bash
iscsi-manager --demo
```
