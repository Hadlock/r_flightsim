# r_flightsim

![r_flightsim logo](https://github.com/hadlock/r_flightsim/blob/master/static/r_flightsim_logo_sm.png)

An basic wireframe flight simulator in rust, using the ggez 2D graphics library

rustc 1.40.0 (rust 2018 edition)
ggez 0.5.1

## Build Requirements

On Ubuntu 18.04 LTS you'll need these packages to build:

- `sudo apt install gcc`
- `sudo apt install libasound2-dev libudev-dev pkg-config`

## Build Improvements

Dev builds are giant, clocking in at over 100MB. Release builds can be under 5MB.

Dev build:

`cargo run`

Release build:

`cargo build --release`
then, `strip target/release/r_flightsim`

```shell
ls -lh target/release/r_flightsim
-rwxrwxr-x   2 hadlock hadlock 4.2M Dec 31 02:52 r_flightsim*
```
