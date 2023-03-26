SIDE = left
CHANNEL = debug
KEYBOARD = butterboard
TARGET_DIRECTORY := target/thumbv7em-none-eabihf/${CHANNEL}
IMAGE_DIRECTORY := images

DEVICE = /dev/sda

all: side

clean:
	cargo clean

compile:
ifeq (${CHANNEL}, "release")
	DEFMT_LOG="trace" KEYBOARD=${KEYBOARD} cargo build --features="${SIDE}" --release
else
	DEFMT_LOG="trace" KEYBOARD=${KEYBOARD} cargo build --features="${SIDE}"
endif

binary:
	mkdir -p ${IMAGE_DIRECTORY}
	arm-none-eabi-objcopy -O binary ${TARGET_DIRECTORY}/butterware ${IMAGE_DIRECTORY}/butterware-${SIDE}.bin

bootloader:
	python tools/uf2conv.py -c -b 0x27000 -f 0xADA52840 ${IMAGE_DIRECTORY}/butterware-${SIDE}.bin -o ${IMAGE_DIRECTORY}/butterware-${SIDE}.uf2

side: compile binary bootloader

both:
	@make SIDE=left
	@make SIDE=right

flash: compile binary bootloader
	sudo mount ${DEVICE} /mnt && sudo cp ${IMAGE_DIRECTORY}/butterware-${SIDE}.uf2 /mnt/ && sudo umount /mnt
