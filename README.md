<img align="left" alt="" src=".github/logo.png" height="130" />

# [Butterware](https://github.com/ve5li/butterware)

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

Butterware is a firmware for split wireless keyboards written entirely in Rust. Internally it uses [Embassy](https://github.com/embassy-rs/embassy), which is a framework for creating embedded applications in Rust. Butterware also heavily utilizes Rust's async/await, which makes it very fast and energy efficient.

# Features
- **RGB lighting**: Support for LEDs that implement the ws2812b protocol using DMA

- **Dynamic master selection** The two halves will dynamically determine which side connects to your device on boot, which helps negate the batteries draining at different speeds.

- **Full Rust** Boards are defined using pure Rust code, which gives you full freedom when trying to add new behaviors or features, while still taking advantage of the the many great features that Rust brings, like memory safety and a strong type system.

- **Easy build system** Since everything is written in Rust the only thing you need to have installed is the Rust tool chain and `arm-none-eabi-objcopy`.

# Building

In contrast to QMK, Butterware is always compiled for a specific side, meaning you need to specify which side to build for. For example, if you wanted to build the firmware for the left side of the Butterboard, you might run

`make SIDE=left KEYBOARD=butterboard`

There is also a helper command to build for both sides at the same time

`make both KEYBOARD=butterboard`

# Flashing

Flashing is very easy when using the [Adafruit nRF52 Bootloader](https://github.com/adafruit/Adafruit_nRF52_Bootloader). Simply connect the board to your device and enter flashing mode by connecting the reset and ground pins twice.
In flash mode the board will then present itself as a storage device and you can simply copy the binary at `images/butterware-<left/right>.uf2` onto the device to flash.


If you are using Linux, you can also use `make flash`. Assuming your board is connected as `/dev/sda`, you may run

`make flash SIDE=left KEYBOARD=butterboard DEVICE=/dev/sda`

It is recommended to always flash both sides to avoid the flash going out of sync, which might cause weird behavior when using the keyboard.
