# Nether Battles

Bare metal Raspberry Pi Dungeon Keeper clone, in a very early development stage.

![Cube with linearly interpolated colors](../../raw/main/cube.jpg)

At the moment the only things this project does are to display a cube with linearly interpolated colors that spins whenever single-finger pan or double-finger rotation gestures are performed on the touchscreen, and plays some tones whose pitch and pan reflect the location of touch points, a hello world of sorts that shows a software 3D rasterizer with perspective correction, lighting, and depth buffering, as well as a software audio stereo synthesizer with pitch, pan, and polyphony, all running on a bare metal (that is, without an operating system) Raspberry Pi 4. The final goal is to turn it into a clone of the original Dungeon Keeper, maybe with support for assets of the game, or maybe with primitive models such as spheres, cylinders, capsules, boxes, cones, as well as either vocal or synthesized sounds, since I'm totally blind and am not an artist.

The purpose of this project is to demonstrate that, although I'm totally blind, that isn't stopping me from writing almost any kind of code, including kernel and computer graphics code, as well as to train myself in hopes to one day reenter the workforce and become an active member of society again.

## Hardware Requirements

* Raspberry Pi 4 Model B
* Official Raspberry Pi Touchscreen
* Stereo headphones or powered speakers

## Building

This project requires a Unix-like system such as Linux or MacOS to build. Although this is a Rust project, I stopped using Cargo because it doesn't support creating a default configuration for building and testing on different targets, so I've included scripts at the root of the project that take care of the building and testing respectively, provided that nightly Rust is installed along with the `rust-src` component. In the future I might release images ready to flash to a storage device using the Raspberry Pi Imager, but meanwhile the recommended way to run this is by configuring a PXE boot service, pointing its TFTP root at this project's `boot` directory, running the `build` script, configuring the Pi's firmware for PXE boot, and booting the Pi from the network.
