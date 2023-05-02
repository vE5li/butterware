SIDE = left
CHANNEL = debug
KEYBOARD = butterboard
DIRECTORY := target/thumbv7em-none-eabihf/${CHANNEL}

DEVICE = /dev/sdb

all: compile binary bootloader

clean:
	cargo clean

compile:
ifeq (${CHANNEL}, "release")
	DEFMT_LOG="trace" KEYBOARD=${KEYBOARD} cargo build --features="${SIDE}" --release
else
	DEFMT_LOG="trace" KEYBOARD=${KEYBOARD} cargo build --features="${SIDE}"
endif

binary:
	arm-none-eabi-objcopy -O binary ${DIRECTORY}/butterware ${DIRECTORY}/butterware-${SIDE}.bin

bootloader:
	python tools/uf2conv.py -c -b 0x27000 -f 0xADA52840 ${DIRECTORY}/butterware-${SIDE}.bin -o ${DIRECTORY}/butterware-${SIDE}.uf2

both:
	@make SIDE=left KEYBOARD=${KEYBOARD} CHANNEL=${CHANNEL} DIRECTORY=${DIRECTORY}
	@make SIDE=right KEYBOARD=${KEYBOARD} CHANNEL=${CHANNEL} DIRECTORY=${DIRECTORY}

flash: compile binary bootloader
	sudo mount ${DEVICE} /mnt && sudo cp target/thumbv7em-none-eabihf/debug/butterware-${SIDE}.uf2 /mnt/ && sudo umount /mnt
