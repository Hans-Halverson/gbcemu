# gbcemu

GameBoy Color emulator, written in Rust for fun.

In development.

## Running

Build and run with `cargo run`. The following command line arguments are supported:

```
Usage: gbcemu [OPTIONS] <ROM>

Arguments:
  <ROM>  ROM file to run

Options:
      --dump-rom    Print info about the ROM to stdout
      --log-frames  Log information about each frame to stdout
      --headless    Run in headless mode (no GUI)
  -h, --help        Print help
```