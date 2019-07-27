# r_flightsim

![openitb logo](https://github.com/hadlock/r_flightsim/blob/master/static/r_flightsim_logo_sm.png)

An basic wireframe flight simulator, using the piston 2D graphics library

## Build

Dev builds are giant, clocking in at over 68MB. Release builds can be under 5MB.

Dev build:

`cargo run`

Release build:

`cargo build --release`
then, `strip ./r_flightsim/target/release/r_flightsim`