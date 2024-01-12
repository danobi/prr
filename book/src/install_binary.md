# Install binary

There are multiple ways to install `prr` CLI tool. Choose any one of the
methods below that best suit your needs.

## Build from source

To build `prr` from source, you will first need to install Rust and Cargo.
Follow the instructions on the [Rust installation page][0].

Once you have installed Rust, the following command can be used to build and
install `prr`:

```sh
cargo install prr
```

This will automatically download `prr` from [crates.io][1], build it, and
install it in Cargo's global binary directory (`~/.cargo/bin/` by default).

To uninstall, run the command:

```sh
cargo uninstall prr
```

## Install using Homebrew

To install `prr` using [Homebrew][2], first follow [Homebrew install
instructions][3].

Once Homebrew is installed, run:

```sh
brew install prr
```


[0]: https://www.rust-lang.org/tools/install
[1]: https://crates.io/
[2]: https://brew.sh/
[3]: https://docs.brew.sh/Installation
