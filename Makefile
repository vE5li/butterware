SIDE = left
CHANNEL = debug
KEYBOARD = butterboard
TARGET_DIRECTORY := target/thumbv7em-none-eabihf/${CHANNEL}
OUTPUT_DIRECTORY := images

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
	mkdir -p ${OUTPUT_DIRECTORY}
	arm-none-eabi-objcopy -O binary ${TARGET_DIRECTORY}/butterware ${OUTPUT_DIRECTORY}/butterware-${SIDE}.bin

bootloader:
	python tools/uf2conv.py -c -b 0x27000 -f 0xADA52840 ${OUTPUT_DIRECTORY}/butterware-${SIDE}.bin -o ${OUTPUT_DIRECTORY}/butterware-${SIDE}.uf2

both:
	@make SIDE=left
	@make SIDE=right

flash: compile binary bootloader
	sudo mount ${DEVICE} /mnt && sudo cp target/thumbv7em-none-eabihf/debug/butterware-${SIDE}.uf2 /mnt/ && sudo umount /mnt
