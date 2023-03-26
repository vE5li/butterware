#!/bin/bash

FLASH_FILE=target/thumbv7em-none-eabihf/$1/firmware.uf2

while :
do
    for device in /dev/sd*; do
        [ -e "$device" ] || continue
        if [[ -z $(findmnt -M "/mnt" | grep "$device") ]]; then
            echo "mounting $device to /mnt"
            mount "$device" /mnt
            echo "flashing $FLASH_FILE"
            cp "$FLASH_FILE" /mnt
            umount /mnt
            echo "unmounted $device from /mnt"
        fi
    done

    sleep 1
done
