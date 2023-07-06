# Nether Battles

Bare metal Raspberry Pi Dungeon Keeper clone, in a very early development stage.

![Triangle with linearly interpolated colors against a farther away gray background square](../../raw/main/triangle.jpg)

At the moment the only thing this project does is to display a gray square in the background with a triangle with linearly interpolated colors that spins whenever two-finger pan or rotation gestures are performed on the touchscreen, a hello world of sorts that shows a basic software 3D rasterizer with perspective correction and depth buffering running on a bare metal (that is, without an operating system) Raspberry Pi 4. The final goal is to turn it into a clone of the original Dungeon Keeper, maybe with support for assets of the game, or maybe with primitive models such as spheres, cylinders, capsules, boxes, cones, as well as either vocal or synthesized sounds, since I'm totally blind and am not an artist, however this might not be possible to accomplish without tapping into the Pi's V3D accelerator, either because my rasterization code sucks or the Raspberry Pi is too slow, as displaying the three triangles required to render the image above at 60 frames per second is already consuming 8% of the Pi's 4 CPU cores, and in its current form my code exhausts the Pi's performance when I make it render about 6600 triangles per second, which is way too low for what I think I need even if the frame rate is reduced to 20 frames per second, and I haven't added lighting yet.

The purpose of this project is to demonstrate that, although I'm totally blind, that isn't stopping me from writing almost any kind of code, including kernel and computer graphics code, as well as to train myself in hopes to one day reenter the workforce and become an active member of society again.

## Hardware Requirements

* Raspberry Pi 4 Model B
* Official Raspberry Pi Touchscreen

## Building

This project requires a Unix-like system such as Linux or MacOS to build. Although this is a Rust project, I have decided to stop using Cargo because it doesn't support creating a default configuration for building and testing on different targets, so I've included scripts at the root of the project that take care of the building and testing respectively, provided that nightly Rust is installed along with the `rust-src` component. In the future I might release images ready to flash to a storage device using the Raspberry Pi Imager, but meanwhile the recommended way to run this is by configuring a PXE boot service, pointing its TFTP root at this project's `boot` directory, running the `build` script, configuring the Pi's firmware for PXE boot, and booting the Pi from the network.
