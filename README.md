# Nether Battles

Bare metal Raspberry Pi Dungeon Keeper clone, in a very early development stage.

![Animated triangle with linearly interpolated colors spinning around the X axis](../../raw/main/triangle.gif)

At the moment the only thing this project does is to display a triangle that spins whenever two-finger pan or rotation gestures are performed on the touchscreen, a hello world of sorts that shows a basic software 3D rasterizer with perspective correction running on a bare metal (that is, without an operating system) Raspberry Pi 4.  The final goal, which I'm not sure I'll be able to accomplish, is to turn it into a clone of the original Dungeon Keeper, maybe with support for assets of the game, or maybe with primitive models such as spheres, cylinders, capsules, boxes, cones, as well as either vocal or synthesized sounds, since I'm totally blind and am not an artist.

The purpose of this project is to demonstrate that, although I'm totally blind, that isn't stopping me from writing almost any kind of code, including kernel and computer graphics code, as well as to train myself in hopes to one day reenter the workforce and become an active member of society again.

## Hardware Requirements

* Raspberry Pi 4 Model B
* Official Raspberry Pi Touchscreen

## Building

This project requires a Unix-like system such as Linux or MacOS to build.  Although this is a Rust project, I have decided to stop using Cargo because  it has some issues with bare metal code, so I've included scripts at the root of the project that take care of the building and testing respectively, provided that nightly Rust is installed along with the `rust-src` component.  In the past I used to provide instructions on how to use a Docker container with all the required software to create an image to flash to a storage device, but since the project isn't that interesting yet and that process is rather complex, I decided to remove that information instead of updating it.  In the future I might release images ready to flash to a storage device using the Raspberry Pi Imager, however if for some odd reason you still want to try this out right now, all the files required to boot the project are placed in the `boot` directory at the root of the project once the build script is executed successfully.
