# Thief: Gold WASM Autosplitter

An autosplitter for Thief: Gold using [Livesplit's](http://livesplit.org/) WASM runtime. This splitter is almost identical in functionality to the [existing splitter](https://github.com/JarrodDoyle/autosplitters/blob/master/thief.asl) with a few exceptions:

- This splitter doesn't have an IL mode
- This splitter forces a split on Constantine's ritual objective
- This splitter only supports NewDark 1.22

The primary reason for developing this new splitter is that I've moved to being 100% Linux based since I previously ran the fullgame categories. I'm wanting to start running fullgame again, but the old ASL style of autosplitting is Windows only. The new WASM runtime has the benefit of working on both Windows and Linux (and Mac I assume), as well as working for both Livesplit and it's upcoming replacement [Livesplit One](https://github.com/LiveSplit/LiveSplitOne).

## How to use with Livesplit

Download the [latest release](https://github.com/JarrodDoyle/lso-thief-gold-splitter/releases), or [build it yourself](#building).

1. Right click the Livesplit window
2. Click `Edit Layout...`
3. Press the `+` button and select `Control->Auto Splitting Runtime`
4. Select the component tab in the layout and choose the WASM file

## Building

To build the splitter you must have the `wasm32-unknown-unknown` target on your Rust toolchain:
```
$ rustup target add wasm32-unknown-unknown
```

Build for release with:
```
$ cargo build --release
```

The built WASM file can be found at `./target/wasm32-unknown-unknown/release/thief_splitter.wasm`.