# Nether Battles

Bare metal Raspberry Pi Dungeon Keeper clone.

![My hand with two extended fingers touching the official Raspberry Pi Touchscreen which is displaying a white ring around each finger on a black background](../../raw/main/rpi-ts.jpg)

At the moment the only thing this project does is to display rings around each touch point on the official Raspberry Pi Touchscreen (pictured above), a "Hello world!" of sorts that demonstrates working video and touchscreen drivers.  The intention is to make this something resembling the original Dungeon Keeper game using colored shapes such as boxes, spheres, cylinders, and capsules as 3D models and vocal sound effects since I'm not an artist and am totally blind.

## Hardware Requirements

* Raspberry Pi 4 Model B
* Official Raspberry Pi Touchscreen
* FTDI TTL-232R-RPI debug cable or similar (optional)

## Building

For your convenience, I've included a `Dockerfile` that sets up a containerized development environment which you can use to compile the code and build the image to flash to an SD card or thumb drive for testing.  However if you intend to tinker with the code I recommend setting up network boot instead.

If you have Docker installed and choose to test that way, the only thing you have to do to set up the project's development environment is to type the following after cloning the project:

    docker build -t nether nether

Then, to get a shell inside the container, type:

    docker run -ti --name nether nether

At this point you should be inside the container at `/root/nether`, so to build the raw binary you must type:

    cargo build --release

In order to boot a Raspberry Pi 4, you will need to create an image with a bootable FAT16 partition, place the files contained in this project's `boot` directory inside, and then flash the resulting image to an SD card or thumb drive.

To create the file that will contain the bootable image, type the following:

    dd if=/dev/zero of=nether.img bs=1M count=8

Then type the following to partition and format the image file:

    echo 'drive c: file="/root/nether/nether.img" partition=1' > /root/.mtoolsrc
    mpartition -Ica c:
    mformat c:

Finally, to copy the contents of this project's `boot` directory to the newly created image, type:

    mcopy boot/* c:

At this point you have a raw image ready to flash to an SD card or thumb drive which you can extract from the container with the `docker cp` command.  Unfortunately since beyond this point things become less portable, and since currently I do not have access to Windows, I will leave the flashing process up to you.

If instead you want to boot from the network, the Raspberry Pi Foundation has published [an official tutorial](https://www.raspberrypi.com/documentation/computers/remote-access.html#network-boot-your-raspberry-pi) on how to do it from another Raspberry Pi.  There's also an option to boot from an HTTP server which you can read about in the [official bootloader configuration documentation](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html#raspberry-pi-4-bootloader-configuration).
