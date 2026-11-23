# Beaver Colony

## Play

### Requirements

- [Rust](https://rustup.rs) 1.97.1 or newer (`rustup update` if you have an older toolchain)
- A GPU with Vulkan, DirectX 12 or Metal support
- Linux only: the usual Bevy system packages, e.g. on Debian/Ubuntu
  `sudo apt install g++ pkg-config libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-0`

### Build

```sh
git clone https://github.com/tristan00/beaver_game.git
cd beaver_game
cargo build --release -p game
```

The first build downloads and compiles dependencies and takes several minutes.

### Launch

Run from the repository root, since saves are written to `saves/` in the current folder.

- **Windows:** double-click `Play Beaver Colony.cmd`
- **Any platform:** `cargo run --release -p game`

Optional arguments (after `--` when using `cargo run`):

| Argument | Effect |
| --- | --- |
| `--map ID` | Start on a specific map (`algonquin`, `telemark`, `voyageurs`, …) or `generated` |
| `--seed N` | Seed for the generated map |
| `--load FILE` | Continue a saved game |
| `--sandbox` | River sandbox on a real map (`River Sandbox.cmd` on Windows) |

Example: `cargo run --release -p game -- --map telemark`
