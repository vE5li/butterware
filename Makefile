CHANNEL = debug
KEYBOARD = meboard
DIRECTORY := target/thumbv7em-none-eabihf/${CHANNEL}

all: compile binary bootloader

clean:
	cargo clean

compile:
ifeq (${CHANNEL}, "release")
	cargo build --no-default-features --features=${KEYBOARD} --release
else
	cargo build --no-default-features --features=${KEYBOARD}
endif

binary:
	arm-none-eabi-objcopy -O binary ${DIRECTORY}/firmware ${DIRECTORY}/firmware.bin

bootloader:
	python tools/uf2conv.py -c -b 0x27000 -f 0xADA52840 ${DIRECTORY}/firmware.bin -o ${DIRECTORY}/firmware.uf2

flash:
	sudo bash tools/flash_helper.sh ${CHANNEL}
