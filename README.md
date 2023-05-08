<img align="left" alt="" src=".github/logo.png" height="130" />

# [Butterware](https://github.com/ve5li/butterware)

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

Butterware is a firmware for split wireless keyboards. This project is supposed to be an alternative to [ZMK](https://github.com/zmkfirmware/zmk), which is powerful but also difficult to work with. The focus of Butterware is on providing a comprehensive feature set, while keeping the firmware easy to understand, configure and extend.


Butterware is written entirely in [Rust](https://www.rust-lang.org/). It is built on top of [Embassy](https://github.com/embassy-rs/embassy), which is a framework for creating embedded applications in Rust. Butterware makes heavy use of Rust's async/await, which allows it to run quickly and efficiently.

# Features
- **RGB lighting**: Butterware supports LEDs that implement the ws2812b protocol.

- **Dynamic master selection**: On boot, the two halves of the keyboard will dynamically determine which side connects to your device. This feature helps prevent one side's batteries from draining faster than the other.

- **Full Rust**: Butterware defines boards using pure Rust code, giving you complete freedom to add new behaviors or features. At the same time, it takes advantage of the many great features that Rust brings, like memory safety and a strong type system.

- **Simple build system**: Since everything is written in Rust, you only need to have the Rust tool chain and `arm-none-eabi-objcopy` installed.

# Building

Unlike QMK, Butterware is always compiled for a specific side, so you need to specify which side to build for. For example, to build the firmware for the left side of the Butterboard, you might run:

`make KEYBOARD=butterboard SIDE=left`

There is also a helper command to build for both sides at the same time:

`make both KEYBOARD=butterboard`

# Flashing

Flashing Butterware is easy when you use the [Adafruit nRF52 Bootloader](https://github.com/adafruit/Adafruit_nRF52_Bootloader). Simply connect the board to your device and enter flashing mode by connecting the reset and ground pins twice. In flash mode, the board presents itself as a storage device. You can then copy the binary at `images/butterware-<left/right>.uf2` onto the device to flash it.

If you are using Linux, you can also use `make flash`. Assuming your board is connected as `/dev/sda`, you may run:

`make flash KEYBOARD=butterboard SIDE=left DEVICE=/dev/sda`

We recommend flashing both sides to avoid the persistent storage going out of sync, which might cause weird behavior when using the keyboard.
